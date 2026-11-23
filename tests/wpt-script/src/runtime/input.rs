//! Events and real input for the generated tests: `addEventListener`, the
//! `Event` a listener receives, and `test_driver` on `TestApp`'s pointer
//! input (RFC 0005, "Real input").
//!
//! `test_driver.Actions` follows `testdriver-actions.js` and Chromium's
//! `testdriver-vendor.js`: each action is its own tick, every coordinate is
//! resolved when `send()` is called, and a touch pointer's moves before its
//! first press are pauses. One tick runs per frame. An action, an option or
//! a precondition this does not cover stops the subtest.

use super::promise::JsPromise;
use super::{Function, Host, Res, Rt, Throw, Value};
use div::{
    Element, Event, EventPhase, ListenerOptions, PointerId, PointerInput, PointerInputKind,
    PointerType,
};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

pub(super) const TESTDRIVER: &str = "resources/testdriver.js";
pub(super) const TESTDRIVER_ACTIONS: &str = "resources/testdriver-actions.js";
pub(super) const POINTEREVENT_SUPPORT: &str = "pointerevents/pointerevent_support.js";

/// The viewport `Rt::new` creates, as `window.innerWidth` and `innerHeight`.
const VIEWPORT: (f64, f64) = (800.0, 600.0);

enum Origin {
    Viewport,
    Element(Element),
}

enum Step {
    Move { x: f64, y: f64, origin: Origin },
    Down,
    Up,
}

/// A `test_driver.Actions` with one pointer source.
#[derive(Default)]
pub struct Actions {
    pointer: Cell<Option<PointerType>>,
    steps: RefCell<Vec<Step>>,
}

/// One tick of a sent sequence: its input (`None` is a pause), and the
/// `send()` promise when it is the last.
pub(super) struct Tick {
    input: Option<PointerInput>,
    done: Option<Rc<JsPromise>>,
}

impl Tick {
    #[cfg(test)]
    pub(super) fn summary(&self) -> Option<(PointerInputKind, f32, f32)> {
        self.input
            .as_ref()
            .map(|input| (input.kind, input.x, input.y))
    }
}

/// A listener added through `addEventListener`, for the DOM's rule that
/// adding the same type and callback again does nothing.
pub(super) struct Registered {
    id: u64,
    element: Element,
    event_type: String,
    callback: Rc<Function>,
}

fn dom_error(surface: &str, error: impl std::fmt::Debug) -> Throw {
    Throw::unsupported(format!("{surface} ({error:?})"))
}

fn no_arguments(surface: &str, args: &[Value]) -> Result<(), Throw> {
    if args.iter().all(|arg| matches!(arg, Value::Undefined)) {
        Ok(())
    } else {
        Err(Throw::unsupported(format!(
            "test_driver:{surface}(options)"
        )))
    }
}

impl Rt {
    // ----- events -----

    /// `element.addEventListener(type, callback, options)`, for a function
    /// callback and no options other than `once`.
    pub(super) fn add_event_listener(&self, element: &Element, args: Vec<Value>) -> Res {
        let mut args = args.into_iter();
        let event_type = self.to_string(&args.next().unwrap_or_default())?;
        let callback = match args.next().unwrap_or_default() {
            Value::Function(callback) => callback,
            Value::Null => return Ok(Value::Undefined),
            other => {
                return Err(Throw::unsupported(format!(
                    "surface:addEventListener({})",
                    other.kind()
                )))
            }
        };
        let once = match args.next().unwrap_or_default() {
            Value::Undefined => false,
            Value::Object(options) => {
                let properties = options.properties.borrow();
                match properties.as_slice() {
                    [] => false,
                    [(key, Value::Bool(once))] if &**key == "once" => *once,
                    _ => return Err(Throw::unsupported("surface:addEventListener(options)")),
                }
            }
            _ => return Err(Throw::unsupported("surface:addEventListener(options)")),
        };
        for listener in self.0.listeners.borrow().iter() {
            let same_node = element
                .is_same_node(&listener.element)
                .map_err(|error| dom_error("addEventListener", error))?;
            if same_node
                && listener.event_type == event_type
                && Rc::ptr_eq(&listener.callback, &callback)
            {
                return Ok(Value::Undefined);
            }
        }
        let id = self.0.listener_order.get() + 1;
        self.0.listener_order.set(id);
        self.0.listeners.borrow_mut().push(Registered {
            id,
            element: element.clone(),
            event_type: event_type.clone(),
            callback: callback.clone(),
        });
        let host = self.clone();
        let function = Value::Function(callback);
        element
            .on_with(
                &event_type,
                ListenerOptions {
                    once,
                    ..ListenerOptions::default()
                },
                move |event| {
                    if once {
                        host.0
                            .listeners
                            .borrow_mut()
                            .retain(|listener| listener.id != id);
                    }
                    let this = event.current_target().map_or(Value::Undefined, |target| {
                        Value::Host(Host::Element(target))
                    });
                    let argument = Value::Host(Host::Event(event.clone()));
                    if let Err(thrown) = host.call(&function, this, vec![argument]) {
                        host.uncaught(thrown);
                    }
                    host.checkpoint();
                },
            )
            .map_err(|error| dom_error("addEventListener", error))?;
        Ok(Value::Undefined)
    }

    /// A member of an `Event`.
    pub(super) fn event_member(&self, event: &Event, key: &str) -> Res {
        match key {
            "type" => Ok(Value::str(&event.event_type)),
            "eventPhase" => Ok(Value::Number(match event.phase() {
                None => 0.0,
                Some(EventPhase::Capture) => 1.0,
                Some(EventPhase::Target) => 2.0,
                Some(EventPhase::Bubble) => 3.0,
            })),
            "target" => Ok(event
                .target()
                .map_or(Value::Null, |target| Value::Host(Host::Element(target)))),
            other => Err(Throw::unsupported(format!("surface:Event.{other}"))),
        }
    }

    /// `getEvent(type, target)` of `pointerevent_support.js`: a promise for
    /// the next such event. Its `test` argument removes the listener at
    /// cleanup, which needs `removeEventListener`.
    pub(super) fn get_event(&self, args: Vec<Value>) -> Res {
        let mut args = args.into_iter();
        let event_type = args.next().unwrap_or_default();
        let target = args.next().unwrap_or_default();
        if self.truthy(&args.next().unwrap_or_default()) {
            return Err(Throw::unsupported("surface:getEvent(test)"));
        }
        let executor = Value::function("", move |rt, _, args| {
            let resolve = args.into_iter().next().unwrap_or_default();
            let listener = Value::function("listener", move |rt, _, args| {
                let event = args.into_iter().next().unwrap_or_default();
                rt.call(&resolve, Value::Undefined, vec![event])
            });
            let options = Value::object(vec![("once", Value::Bool(true))]);
            rt.call_method(
                &target,
                "addEventListener",
                vec![event_type.clone(), listener, options],
            )
        });
        self.construct(&Value::Host(Host::Constructor("Promise")), vec![executor])
    }

    // ----- test_driver -----

    /// A member of the `test_driver` global.
    pub(super) fn test_driver_member(&self, key: &str) -> Res {
        match key {
            "Actions" if self.has_helper(TESTDRIVER_ACTIONS) => {
                Ok(Value::Host(Host::Constructor("test_driver.Actions")))
            }
            other => Err(Throw::unsupported(format!("surface:test_driver.{other}"))),
        }
    }

    /// `new test_driver.Actions()`.
    pub(super) fn construct_actions(&self, args: Vec<Value>) -> Res {
        no_arguments("Actions", &args)?;
        Ok(Value::Host(Host::Actions(Rc::new(Actions::default()))))
    }

    /// A method of a `test_driver.Actions`. The builders return the object.
    pub(super) fn actions_member(&self, actions: &Rc<Actions>, key: &str) -> Res {
        let actions = actions.clone();
        let this = Value::Host(Host::Actions(actions.clone()));
        Ok(match key {
            "addPointer" => Value::function("addPointer", move |rt, _, args| {
                actions.add_pointer(rt, &args)?;
                Ok(this.clone())
            }),
            "pointerMove" => Value::function("pointerMove", move |rt, _, args| {
                let step = rt.move_step(&args)?;
                actions.steps.borrow_mut().push(step);
                Ok(this.clone())
            }),
            "pointerDown" => Value::function("pointerDown", move |_, _, args| {
                no_arguments("pointerDown", &args)?;
                actions.steps.borrow_mut().push(Step::Down);
                Ok(this.clone())
            }),
            "pointerUp" => Value::function("pointerUp", move |_, _, args| {
                no_arguments("pointerUp", &args)?;
                actions.steps.borrow_mut().push(Step::Up);
                Ok(this.clone())
            }),
            "send" => Value::function("send", move |rt, _, _| rt.send_actions(&actions)),
            other => return Err(Throw::unsupported(format!("surface:Actions.{other}"))),
        })
    }

    fn move_step(&self, args: &[Value]) -> Result<Step, Throw> {
        let (Some(Value::Number(x)), Some(Value::Number(y))) = (args.first(), args.get(1)) else {
            return Err(Throw::unsupported("test_driver:pointerMove(coordinates)"));
        };
        let origin = match args.get(2) {
            None | Some(Value::Undefined) => Origin::Viewport,
            Some(Value::Object(options)) => {
                let properties = options.properties.borrow();
                match properties.as_slice() {
                    [(key, Value::Host(Host::Element(element)))] if &**key == "origin" => {
                        Origin::Element(element.clone())
                    }
                    _ => return Err(Throw::unsupported("test_driver:pointerMove(options)")),
                }
            }
            Some(_) => return Err(Throw::unsupported("test_driver:pointerMove(options)")),
        };
        if args.len() > 3 {
            return Err(Throw::unsupported("test_driver:pointerMove(options)"));
        }
        Ok(Step::Move {
            x: *x,
            y: *y,
            origin,
        })
    }

    /// The vendor file's point for an element origin: the in-view centre of
    /// its first client rect. It scrolls an element that is out of view and
    /// rejects one something else covers; neither is translated.
    fn origin_point(&self, element: &Element) -> Result<(f64, f64), Throw> {
        let connected = element
            .is_connected()
            .map_err(|error| dom_error("test_driver:origin", error))?;
        if !connected {
            return Err(Throw::error(
                "Error: element in different document or iframe",
            ));
        }
        let rects = element
            .client_rects()
            .map_err(|error| dom_error("test_driver:origin", error))?;
        let Some(rect) = rects.first() else {
            return Err(Throw::unsupported("test_driver:origin-not-in-view"));
        };
        let (rect_x, rect_y) = (f64::from(rect.x), f64::from(rect.y));
        let left = rect_x.max(0.0);
        let right = VIEWPORT.0.min(rect_x + f64::from(rect.width));
        let top = rect_y.max(0.0);
        let bottom = VIEWPORT.1.min(rect_y + f64::from(rect.height));
        let (x, y) = (0.5 * (left + right), 0.5 * (top + bottom));
        let hit = self
            .document()
            .hit_test(x as f32, y as f32)
            .map_err(|error| dom_error("test_driver:origin", error))?;
        let Some(hit) = hit else {
            return Err(Throw::unsupported("test_driver:origin-not-in-view"));
        };
        let reached = element
            .contains(&hit)
            .map_err(|error| dom_error("test_driver:origin", error))?;
        if !reached {
            return Err(Throw::unsupported("test_driver:origin-covered"));
        }
        Ok((x, y))
    }

    /// `send()`: the sequence's ticks, with every coordinate resolved now.
    fn send_actions(&self, actions: &Actions) -> Res {
        let promise = self.new_promise();
        match self.ticks(actions, &promise) {
            Ok(ticks) => *self.0.pending_input.borrow_mut() = ticks,
            // The driver's own rejection, which the page can see.
            Err(Throw::Error(message)) => self.settle(&promise, Err(Throw::Error(message))),
            Err(unsupported) => return Err(unsupported),
        }
        Ok(Value::Host(Host::Promise(promise)))
    }

    fn ticks(&self, actions: &Actions, promise: &Rc<JsPromise>) -> Result<VecDeque<Tick>, Throw> {
        if !self.0.pending_input.borrow().is_empty() {
            return Err(Throw::unsupported("test_driver:overlapping-sequences"));
        }
        let steps = actions.steps.borrow();
        if steps.is_empty() {
            return Err(Throw::unsupported("test_driver:empty-sequence"));
        }
        let pointer_type = actions.pointer.get().unwrap_or(PointerType::Mouse);
        let pointer_id = match pointer_type {
            PointerType::Mouse => PointerId::MOUSE,
            PointerType::Pen | PointerType::Touch => PointerId(2),
        };
        let input = |kind, x: f64, y: f64| {
            let mut input = PointerInput::touch(pointer_id, kind, x as f32, y as f32);
            input.pointer_type = pointer_type;
            input
        };
        let (mut x, mut y) = (0.0, 0.0);
        let mut pressed = false;
        let mut ticks = VecDeque::with_capacity(steps.len());
        for step in steps.iter() {
            let tick = match step {
                Step::Move {
                    x: move_x,
                    y: move_y,
                    origin,
                } => {
                    let (origin_x, origin_y) = match origin {
                        Origin::Viewport => (0.0, 0.0),
                        Origin::Element(element) => self.origin_point(element)?,
                    };
                    x = move_x + origin_x;
                    y = move_y + origin_y;
                    if !x.is_finite() || !y.is_finite() {
                        return Err(Throw::unsupported("test_driver:pointerMove(coordinates)"));
                    }
                    (pointer_type != PointerType::Touch || pressed)
                        .then(|| input(PointerInputKind::Move, x, y))
                }
                Step::Down => {
                    pressed = true;
                    Some(input(PointerInputKind::Down, x, y))
                }
                Step::Up => {
                    pressed = false;
                    Some(input(PointerInputKind::Up, x, y))
                }
            };
            ticks.push_back(Tick {
                input: tick,
                done: None,
            });
        }
        if let Some(last) = ticks.back_mut() {
            last.done = Some(promise.clone());
        }
        Ok(ticks)
    }

    /// This frame's tick of the sequence being sent, if any.
    pub(super) fn run_input_tick(&self) {
        let tick = self.0.pending_input.borrow_mut().pop_front();
        let Some(tick) = tick else { return };
        if let Some(input) = tick.input {
            // `TestApp::pointer`, without its panic: a dispatch Div fails is
            // the page's error, recorded with the results.
            let dispatched = self.document().dispatch_pointer_input(input);
            self.0.app.borrow_mut().run_ready_tasks();
            self.checkpoint();
            if let Err(error) = dispatched {
                self.uncaught(Throw::error(format!(
                    "Div: pointer input failed ({error:?})"
                )));
            }
        }
        if let Some(done) = tick.done {
            self.resolve_promise(&done, Value::Undefined);
            self.checkpoint();
        }
    }
}

impl Actions {
    /// `addPointer(name, pointerType)`, for the sequence's only pointer.
    fn add_pointer(&self, rt: &Rt, args: &[Value]) -> Result<(), Throw> {
        if !matches!(args.first(), Some(Value::Str(_))) || args.len() > 2 {
            return Err(Throw::unsupported("test_driver:addPointer(arguments)"));
        }
        if self.pointer.get().is_some() || !self.steps.borrow().is_empty() {
            return Err(Throw::unsupported("test_driver:multiple-pointers"));
        }
        let name = match args.get(1) {
            None | Some(Value::Undefined) => "mouse".to_owned(),
            Some(value) => rt.to_string(value)?,
        };
        self.pointer.set(Some(match name.as_str() {
            "mouse" => PointerType::Mouse,
            "pen" => PointerType::Pen,
            "touch" => PointerType::Touch,
            other => {
                return Err(Throw::unsupported(format!(
                    "test_driver:pointerType:{other}"
                )))
            }
        }));
        Ok(())
    }
}
