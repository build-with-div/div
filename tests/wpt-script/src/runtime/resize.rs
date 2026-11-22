//! `ResizeObserver` on `divui`'s, and WPT's `resizeTestHelper.js` as an
//! intrinsic: the helper's control flow, statement for statement, for the
//! sha256 `tools/wpt_script/intrinsics.json` pins.
//!
//! One difference is deliberate. The helper's `catch` turns any throw into a
//! test failure and carries on; an unsupported operation is not a throw the
//! page could see, so it finishes the test as UNSUPPORTED and stops the
//! page's chain instead of running later tests from an unvouched state.

use super::promise::JsPromise;
use super::{number_to_string, Host, Res, Rt, Throw, Value};
use div::{
    AnimationFrameHandle, Rect, ResizeObserver, ResizeObserverBox, ResizeObserverEntry,
    ResizeObserverSize,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// `ResizeTestHelper.TIMEOUT`.
const TIMEOUT_MS: i32 = 1000;
/// `ResizeTestHelper.SHORT_TIMEOUT`.
const SHORT_TIMEOUT_MS: i32 = 100;

/// A `ResizeTestHelper` object.
pub struct Helper {
    name: Value,
    steps: Value,
    step_index: Cell<f64>,
    harness_test: Cell<Option<usize>>,
    observer: RefCell<Option<Value>>,
    timeout_id: Cell<Option<u64>>,
    promise: RefCell<Option<Rc<JsPromise>>>,
    raf_request: RefCell<Option<AnimationFrameHandle>>,
    raf_count: Cell<f64>,
}

impl Helper {
    fn observer(&self) -> Value {
        self.observer.borrow().clone().unwrap_or_default()
    }

    fn test(&self) -> Result<usize, Throw> {
        self.harness_test
            .get()
            .ok_or_else(|| Throw::error("TypeError: _harnessTest is null"))
    }
}

/// A `ResizeObserverSize`, in the frozen one-item array the web reports.
fn sizes(size: ResizeObserverSize) -> Value {
    Value::array(vec![Value::record(
        "ResizeObserverSize",
        vec![
            ("inlineSize", Value::Number(size.inline_size)),
            ("blockSize", Value::Number(size.block_size)),
        ],
    )])
}

/// A `DOMRectReadOnly`.
fn rect(rect: Rect) -> Value {
    let (x, y, width, height) = (
        f64::from(rect.x),
        f64::from(rect.y),
        f64::from(rect.width),
        f64::from(rect.height),
    );
    Value::record(
        "DOMRectReadOnly",
        vec![
            ("x", Value::Number(x)),
            ("y", Value::Number(y)),
            ("width", Value::Number(width)),
            ("height", Value::Number(height)),
            ("top", Value::Number(y.min(y + height))),
            ("right", Value::Number(x.max(x + width))),
            ("bottom", Value::Number(y.max(y + height))),
            ("left", Value::Number(x.min(x + width))),
        ],
    )
}

fn entry(entry: &ResizeObserverEntry) -> Value {
    Value::record(
        "ResizeObserverEntry",
        vec![
            ("target", Value::Host(Host::Element(entry.target.clone()))),
            ("contentRect", rect(entry.content_rect)),
            ("borderBoxSize", sizes(entry.border_box_size)),
            ("contentBoxSize", sizes(entry.content_box_size)),
            (
                "devicePixelContentBoxSize",
                sizes(entry.device_pixel_content_box_size),
            ),
        ],
    )
}

fn target(method: &str, value: Option<&Value>) -> Result<div::Element, Throw> {
    match value {
        Some(Value::Host(Host::Element(element))) => Ok(element.clone()),
        _ => Err(Throw::error(format!(
            "TypeError: Failed to execute '{method}' on 'ResizeObserver': parameter 1 is not of type 'Element'."
        ))),
    }
}

/// WebIDL's conversion of `ResizeObserverOptions`.
fn observed_box(rt: &Rt, options: Option<&Value>) -> Result<ResizeObserverBox, Throw> {
    let value = match options {
        None | Some(Value::Undefined | Value::Null) => return Ok(ResizeObserverBox::ContentBox),
        Some(options @ Value::Object(_)) => rt.get(options, "box")?,
        Some(_) => {
            return Err(Throw::error(
                "TypeError: Failed to execute 'observe' on 'ResizeObserver': The provided value is not of type 'ResizeObserverOptions'.",
            ))
        }
    };
    if matches!(value, Value::Undefined) {
        return Ok(ResizeObserverBox::ContentBox);
    }
    match rt.to_string(&value)?.as_str() {
        "content-box" => Ok(ResizeObserverBox::ContentBox),
        "border-box" => Ok(ResizeObserverBox::BorderBox),
        "device-pixel-content-box" => Ok(ResizeObserverBox::DevicePixelContentBox),
        other => Err(Throw::error(format!(
            "TypeError: Failed to read the 'box' property from 'ResizeObserverOptions': The provided value '{other}' is not a valid enum value of type ResizeObserverBoxOptions."
        ))),
    }
}

impl Rt {
    /// `new ResizeObserver(callback)`. The web keeps an observer with
    /// targets alive; the runtime holds every observer until the page ends.
    pub(super) fn construct_resize_observer(&self, args: Vec<Value>) -> Res {
        let callback = args.into_iter().next().unwrap_or_default();
        if !matches!(callback, Value::Function(_)) {
            return Err(Throw::error(
                "TypeError: Failed to construct 'ResizeObserver': parameter 1 is not of type 'Function'.",
            ));
        }
        let host = self.clone();
        let observer = ResizeObserver::new(self.document(), move |entries, observer| {
            let entries = Value::array(entries.iter().map(entry).collect());
            let this = Value::Host(Host::ResizeObserver(observer.clone()));
            if let Err(thrown) = host.call(&callback, this.clone(), vec![entries, this]) {
                host.uncaught(thrown);
            }
            host.checkpoint();
        });
        self.0.observers.borrow_mut().push(observer.clone());
        Ok(Value::Host(Host::ResizeObserver(observer)))
    }

    /// A member of a `ResizeObserver`.
    pub(super) fn resize_observer_member(&self, observer: &ResizeObserver, key: &str) -> Res {
        let observer = observer.clone();
        match key {
            "observe" => Ok(Value::function("observe", move |rt, _, args| {
                let element = target("observe", args.first())?;
                let observed = observed_box(rt, args.get(1))?;
                observer.observe(&element, observed);
                Ok(Value::Undefined)
            })),
            "unobserve" => Ok(Value::function("unobserve", move |_, _, args| {
                observer.unobserve(&target("unobserve", args.first())?);
                Ok(Value::Undefined)
            })),
            "disconnect" => Ok(Value::function("disconnect", move |_, _, _| {
                observer.disconnect();
                Ok(Value::Undefined)
            })),
            other => Err(Throw::unsupported(format!(
                "surface:ResizeObserver.{other}"
            ))),
        }
    }

    /// `createAndAppendElement(tagName, parent)`.
    pub(super) fn create_and_append_element(&self, args: Vec<Value>) -> Res {
        let parent = match args.get(1) {
            Some(parent) if self.truthy(parent) => parent.clone(),
            _ => self.get(&Value::Host(Host::Document), "body")?,
        };
        let element = Value::Host(Host::Element(
            self.create_element(&args.first().cloned().unwrap_or_default())?,
        ));
        self.call_method(&parent, "appendChild", vec![element.clone()])?;
        Ok(element)
    }

    /// `new ResizeTestHelper(name, steps)`.
    pub(super) fn construct_helper(&self, args: Vec<Value>) -> Res {
        let mut args = args.into_iter();
        let name = args.next().unwrap_or_default();
        let steps = args.next().unwrap_or_default();
        let steps = if self.truthy(&steps) {
            steps
        } else {
            Value::array(Vec::new())
        };
        let helper = Rc::new(Helper {
            name,
            steps,
            step_index: Cell::new(-1.0),
            harness_test: Cell::new(None),
            observer: RefCell::new(None),
            timeout_id: Cell::new(None),
            promise: RefCell::new(None),
            raf_request: RefCell::new(None),
            raf_count: Cell::new(0.0),
        });
        let bound = helper.clone();
        let handler = Value::function("_handleNotification", move |rt, _, args| {
            rt.helper_notification(&bound, args.into_iter().next().unwrap_or_default())
        });
        let observer = self.construct_resize_observer(vec![handler])?;
        *helper.observer.borrow_mut() = Some(observer);
        Ok(Value::Host(Host::Helper(helper)))
    }

    /// A member of a `ResizeTestHelper`.
    pub(super) fn helper_member(&self, helper: &Rc<Helper>, key: &str) -> Res {
        let helper = helper.clone();
        match key {
            "start" => Ok(Value::function("start", move |rt, _, args| {
                rt.helper_start(&helper, args.into_iter().next().unwrap_or_default())
            })),
            "startCountingRaf" => Ok(Value::function("startCountingRaf", move |rt, _, _| {
                if let Some(request) = helper.raf_request.borrow_mut().take() {
                    rt.document().cancel_animation_frame(request);
                }
                helper.raf_count.set(0.0);
                rt.helper_request_count(&helper);
                Ok(Value::Undefined)
            })),
            "rafCount" => {
                if helper.raf_request.borrow().is_none() {
                    return Err(Throw::error("rAF count is not active"));
                }
                Ok(Value::Number(helper.raf_count.get()))
            }
            "test" => match helper.harness_test.get() {
                Some(index) => Ok(Value::Host(Host::Test(index))),
                None => Err(Throw::error("_harnessTest is not initialized")),
            },
            other => Err(Throw::unsupported(format!(
                "surface:ResizeTestHelper.{other}"
            ))),
        }
    }

    /// `_incrementRaf`'s request.
    fn helper_request_count(&self, helper: &Rc<Helper>) {
        let bound = helper.clone();
        let request = self.native_frame(move |rt| {
            if bound.raf_request.borrow().is_some() {
                bound.raf_count.set(bound.raf_count.get() + 1.0);
                rt.helper_request_count(&bound);
            }
            Ok(())
        });
        *helper.raf_request.borrow_mut() = Some(request);
    }

    /// `harnessTest.step(body)`, where an unsupported operation also stops
    /// the page's chain.
    fn helper_step(&self, test: usize, body: impl Fn(&Rt) -> Result<(), Throw>) {
        if self.is_finished(test) {
            return;
        }
        if let Err(thrown) = body(self) {
            if let Throw::Unsupported(reason) = &thrown {
                self.stall(reason);
            }
            self.finish_thrown(test, thrown);
        }
    }

    /// `start(cleanup)`.
    fn helper_start(&self, helper: &Rc<Helper>, cleanup: Value) -> Res {
        let name = helper.name.clone();
        let test = self.new_test(Some(&name))?;
        helper.harness_test.set(Some(test));
        if self.truthy(&cleanup) {
            self.add_cleanup(test, cleanup);
        }
        self.helper_step(test, |rt| {
            let index = helper.step_index.get();
            if index != -1.0 {
                return Err(Throw::error(format!(
                    "assert_equals: start can only be called once expected -1 but got {}",
                    number_to_string(index)
                )));
            }
            rt.helper_next_step(helper)
        });
        let promise = self.new_promise();
        *helper.promise.borrow_mut() = Some(promise.clone());
        Ok(Value::Host(Host::Promise(promise)))
    }

    /// `_currentStep`.
    fn helper_current(&self, helper: &Helper) -> Res {
        let index = helper.step_index.get();
        if index < 0.0 {
            return Ok(Value::Undefined);
        }
        self.get(&helper.steps, &number_to_string(index))
    }

    /// `_nextStep`.
    fn helper_next_step(&self, helper: &Rc<Helper>) -> Result<(), Throw> {
        let index = helper.step_index.get() + 1.0;
        helper.step_index.set(index);
        let length = self.to_number(&self.get(&helper.steps, "length")?)?;
        if index == length {
            return self.helper_done(helper);
        }
        let step = self.helper_current(helper)?;
        let delay = if self.truthy(&self.get(&step, "timeout")?) {
            SHORT_TIMEOUT_MS
        } else {
            TIMEOUT_MS
        };
        let test = helper.test()?;
        let bound = helper.clone();
        let handler = Value::function("_handleTimeout", move |rt, _, _| {
            rt.helper_timeout(&bound)?;
            Ok(Value::Undefined)
        });
        let id = self.set_timer(
            handler,
            Value::Host(Host::Test(test)),
            Vec::new(),
            delay,
            Some(test),
        )?;
        helper.timeout_id.set(Some(id));
        match self.call_method(&step, "setup", vec![helper.observer()]) {
            Ok(_) => Ok(()),
            // Not a throw the page could catch.
            Err(thrown @ Throw::Unsupported(_)) => Err(thrown),
            Err(Throw::Error(_)) => {
                self.helper_step(test, |_| {
                    Err(Throw::error(
                        "assert_unreached: Caught a throw, possible syntax error Reached unreachable code",
                    ))
                });
                Ok(())
            }
        }
    }

    /// `_handleNotification(entries)`.
    fn helper_notification(&self, helper: &Rc<Helper>, entries: Value) -> Res {
        if let Some(id) = helper.timeout_id.take() {
            self.clear_timer(id);
        }
        let test = helper.test()?;
        self.helper_step(test, |rt| {
            let outcome = (|| -> Result<(), Throw> {
                let step = rt.helper_current(helper)?;
                let delay =
                    rt.call_method(&step, "notify", vec![entries.clone(), helper.observer()])?;
                if rt.truthy(&delay) {
                    let bound = helper.clone();
                    rt.native_frame(move |rt| rt.helper_next_step(&bound));
                    Ok(())
                } else {
                    rt.helper_next_step(helper)
                }
            })();
            match outcome {
                Ok(()) => Ok(()),
                Err(thrown @ Throw::Unsupported(_)) => Err(thrown),
                Err(thrown) => {
                    // `step(() => { throw err; })`, then force `_done()`.
                    rt.finish_thrown(test, thrown);
                    rt.helper_done(helper)
                }
            }
        });
        Ok(Value::Undefined)
    }

    /// `_handleTimeout`.
    fn helper_timeout(&self, helper: &Rc<Helper>) -> Result<(), Throw> {
        helper.timeout_id.set(None);
        let test = helper.test()?;
        self.helper_step(test, |rt| {
            let step = rt.helper_current(helper)?;
            let timeout = rt.get(&step, "timeout")?;
            if rt.truthy(&timeout) {
                rt.call(&timeout, step, Vec::new())?;
            } else {
                rt.helper_step(test, |_| {
                    Err(Throw::error(format!(
                        "assert_unreached: Timed out waiting for notification. ({TIMEOUT_MS}ms) Reached unreachable code"
                    )))
                });
            }
            rt.helper_next_step(helper)
        });
        Ok(())
    }

    /// `_done`.
    fn helper_done(&self, helper: &Rc<Helper>) -> Result<(), Throw> {
        let observer = helper.observer.borrow_mut().take();
        let Some(Value::Host(Host::ResizeObserver(observer))) = observer else {
            return Err(Throw::error(
                "TypeError: Cannot read properties of undefined (reading 'disconnect')",
            ));
        };
        observer.disconnect();
        self.done(helper.test()?);
        if let Some(request) = helper.raf_request.borrow_mut().take() {
            self.document().cancel_animation_frame(request);
        }
        let bound = helper.clone();
        self.native_frame(move |rt| {
            let promise = bound.promise.borrow().clone();
            let Some(promise) = promise else {
                return Err(Throw::error(
                    "TypeError: this._resolvePromise is not a function",
                ));
            };
            rt.resolve_promise(&promise, Value::Undefined);
            Ok(())
        });
        Ok(())
    }
}
