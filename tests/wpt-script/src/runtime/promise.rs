//! ECMAScript promises, async functions and testharness.js's `promise_test`.
//!
//! The job order follows ECMA-262: a reaction is one microtask, and resolving
//! a promise with a promise goes through `NewPromiseResolveThenableJob`, so
//! the number of ticks between two callbacks is the web's. What the runtime
//! cannot carry exactly is rejected by name: a rejection handler (rejection
//! reasons are not script values here), `reject(value)`, and any thenable
//! that is not a promise.

use super::{Host, Res, Rt, Status, Throw, Value};
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

/// How a promise settled.
pub(super) type Settled = Result<Value, Throw>;
type Reaction = Box<dyn FnOnce(&Rt, Settled)>;
/// A job of the microtask queue.
pub(super) type Microtask = Box<dyn FnOnce(&Rt)>;

enum State {
    Pending(Vec<Reaction>),
    Fulfilled(Value),
    Rejected(Throw),
}

/// A JavaScript `Promise`.
pub struct JsPromise {
    state: RefCell<State>,
    /// `[[PromiseIsHandled]]`.
    handled: Cell<bool>,
}

/// A running async function: its body, and the promise it settles.
pub(super) struct Task {
    future: RefCell<Option<Pin<Box<dyn Future<Output = Res>>>>>,
    promise: Rc<JsPromise>,
}

/// `await value`.
pub struct Await {
    rt: Rt,
    value: Option<Value>,
    result: Rc<RefCell<Option<Settled>>>,
}

impl Future for Await {
    type Output = Res;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Res> {
        let this = self.get_mut();
        if let Some(value) = this.value.take() {
            let promise = this.rt.promise_resolve(value);
            let task = this
                .rt
                .0
                .current_task
                .borrow()
                .clone()
                .expect("`await` runs inside an async function's task");
            let result = this.result.clone();
            this.rt.add_reaction(
                &promise,
                Box::new(move |rt, settled| {
                    *result.borrow_mut() = Some(settled);
                    rt.poll_task(&task);
                }),
            );
            return Poll::Pending;
        }
        match this.result.borrow_mut().take() {
            Some(settled) => Poll::Ready(settled),
            None => Poll::Pending,
        }
    }
}

/// Tasks are resumed by promise reactions, never by a waker.
struct NoWake;

impl Wake for NoWake {
    fn wake(self: Arc<Self>) {}
}

impl Rt {
    pub(super) fn new_promise(&self) -> Rc<JsPromise> {
        Rc::new(JsPromise {
            state: RefCell::new(State::Pending(Vec::new())),
            handled: Cell::new(false),
        })
    }

    pub(super) fn enqueue(&self, task: Microtask) {
        self.0.microtasks.borrow_mut().push_back(task);
    }

    /// HTML's "perform a microtask checkpoint", then its "notify about
    /// rejected promises". Called when a script or callback has finished
    /// and the stack is empty.
    pub fn checkpoint(&self) {
        if self.0.in_checkpoint.replace(true) {
            return;
        }
        loop {
            let task = self.0.microtasks.borrow_mut().pop_front();
            let Some(task) = task else { break };
            task(self);
        }
        self.0.in_checkpoint.set(false);
        let rejected = std::mem::take(&mut *self.0.rejections.borrow_mut());
        for promise in rejected {
            if promise.handled.get() {
                continue;
            }
            let thrown = match &*promise.state.borrow() {
                State::Rejected(thrown) => thrown.clone(),
                _ => continue,
            };
            self.unhandled_rejection(thrown);
        }
    }

    /// FulfillPromise or RejectPromise. A promise settles once.
    pub(super) fn settle(&self, promise: &Rc<JsPromise>, result: Settled) {
        let reactions = {
            let mut state = promise.state.borrow_mut();
            if !matches!(&*state, State::Pending(_)) {
                return;
            }
            let settled = match &result {
                Ok(value) => State::Fulfilled(value.clone()),
                Err(thrown) => State::Rejected(thrown.clone()),
            };
            match std::mem::replace(&mut *state, settled) {
                State::Pending(reactions) => reactions,
                _ => unreachable!("the state was pending"),
            }
        };
        if result.is_err() && !promise.handled.get() {
            self.0.rejections.borrow_mut().push(promise.clone());
        }
        for reaction in reactions {
            let result = result.clone();
            self.enqueue(Box::new(move |rt| reaction(rt, result)));
        }
    }

    /// A promise resolve function's steps (ECMA-262 27.2.1.3.2).
    pub(super) fn resolve_promise(&self, promise: &Rc<JsPromise>, value: Value) {
        let thenable = match &value {
            Value::Host(Host::Promise(inner)) if Rc::ptr_eq(inner, promise) => {
                self.settle(
                    promise,
                    Err(Throw::error(
                        "TypeError: Chaining cycle detected for promise #<Promise>",
                    )),
                );
                return;
            }
            Value::Host(Host::Promise(inner)) => Some(inner.clone()),
            Value::Object(object) if object.own("then").is_some() => {
                self.settle(promise, Err(Throw::unsupported("thenable:object")));
                return;
            }
            // A named element or a script could give these a `then`.
            Value::Host(Host::Window | Host::Document) => {
                self.settle(promise, Err(Throw::unsupported("thenable:host")));
                return;
            }
            _ => None,
        };
        let Some(inner) = thenable else {
            self.settle(promise, Ok(value));
            return;
        };
        // NewPromiseResolveThenableJob: `then` runs a microtask later.
        let promise = promise.clone();
        self.enqueue(Box::new(move |rt| {
            rt.add_reaction(
                &inner,
                Box::new(move |rt, settled| match settled {
                    Ok(value) => rt.resolve_promise(&promise, value),
                    Err(thrown) => rt.settle(&promise, Err(thrown)),
                }),
            );
        }));
    }

    /// PerformPromiseThen with a native reaction.
    pub(super) fn add_reaction(&self, promise: &Rc<JsPromise>, reaction: Reaction) {
        promise.handled.set(true);
        let settled = match &mut *promise.state.borrow_mut() {
            State::Pending(reactions) => {
                reactions.push(reaction);
                return;
            }
            State::Fulfilled(value) => Ok(value.clone()),
            State::Rejected(thrown) => Err(thrown.clone()),
        };
        self.enqueue(Box::new(move |rt| reaction(rt, settled)));
    }

    /// `PromiseResolve(%Promise%, value)`.
    pub(super) fn promise_resolve(&self, value: Value) -> Rc<JsPromise> {
        if let Value::Host(Host::Promise(promise)) = &value {
            return promise.clone();
        }
        let promise = self.new_promise();
        self.resolve_promise(&promise, value);
        promise
    }

    fn promise_then(&self, promise: &Rc<JsPromise>, args: Vec<Value>) -> Res {
        let mut args = args.into_iter();
        let on_fulfilled = args.next().unwrap_or_default();
        if !matches!(args.next(), None | Some(Value::Undefined)) {
            return Err(Throw::unsupported(
                "surface:Promise.prototype.then(onRejected)",
            ));
        }
        let derived = self.new_promise();
        let result = derived.clone();
        self.add_reaction(
            promise,
            Box::new(move |rt, settled| match settled {
                Ok(value) if matches!(on_fulfilled, Value::Function(_)) => {
                    match rt.call(&on_fulfilled, Value::Undefined, vec![value]) {
                        Ok(value) => rt.resolve_promise(&derived, value),
                        Err(thrown) => rt.settle(&derived, Err(thrown)),
                    }
                }
                // A handler that is not callable passes the result through.
                settled => rt.settle(&derived, settled),
            }),
        );
        Ok(Value::Host(Host::Promise(result)))
    }

    /// A member of a promise object.
    pub(super) fn promise_member(&self, promise: &Rc<JsPromise>, key: &str) -> Res {
        match key {
            "then" => {
                let promise = promise.clone();
                Ok(Value::function("then", move |rt, _, args| {
                    rt.promise_then(&promise, args)
                }))
            }
            other => Err(Throw::unsupported(format!(
                "surface:Promise.prototype.{other}"
            ))),
        }
    }

    /// A static member of `Promise`.
    pub(super) fn promise_static(&self, key: &str) -> Res {
        match key {
            "resolve" => Ok(Value::function("resolve", |rt, _, args| {
                let value = args.into_iter().next().unwrap_or_default();
                Ok(Value::Host(Host::Promise(rt.promise_resolve(value))))
            })),
            other => Err(Throw::unsupported(format!("surface:Promise.{other}"))),
        }
    }

    /// `new Promise(executor)`.
    pub(super) fn construct_promise(&self, args: Vec<Value>) -> Res {
        let executor = args.into_iter().next().unwrap_or_default();
        if !matches!(executor, Value::Function(_)) {
            return Err(Throw::error(format!(
                "TypeError: Promise resolver {executor} is not a function"
            )));
        }
        let promise = self.new_promise();
        let already_resolved = Rc::new(Cell::new(false));
        let resolve = {
            let promise = promise.clone();
            let already_resolved = already_resolved.clone();
            Value::function("", move |rt, _, args| {
                if !already_resolved.replace(true) {
                    let value = args.into_iter().next().unwrap_or_default();
                    rt.resolve_promise(&promise, value);
                }
                Ok(Value::Undefined)
            })
        };
        let reject = {
            let promise = promise.clone();
            let already_resolved = already_resolved.clone();
            Value::function("", move |rt, _, _| {
                if !already_resolved.replace(true) {
                    // A rejection reason is a script value, which the
                    // runtime does not carry.
                    rt.settle(&promise, Err(Throw::unsupported("promise-rejection-value")));
                }
                Ok(Value::Undefined)
            })
        };
        if let Err(thrown) = self.call(&executor, Value::Undefined, vec![resolve, reject]) {
            if !already_resolved.replace(true) {
                self.settle(&promise, Err(thrown));
            }
        }
        Ok(Value::Host(Host::Promise(promise)))
    }

    /// Start an async function's body; its promise is the call's value. The
    /// body runs synchronously up to its first `await`.
    pub fn spawn_async(&self, body: impl Future<Output = Res> + 'static) -> Res {
        let promise = self.new_promise();
        let task = Rc::new(Task {
            future: RefCell::new(Some(Box::pin(body))),
            promise: promise.clone(),
        });
        self.poll_task(&task);
        Ok(Value::Host(Host::Promise(promise)))
    }

    fn poll_task(&self, task: &Rc<Task>) {
        let Some(mut future) = task.future.borrow_mut().take() else {
            return;
        };
        let previous = self.0.current_task.replace(Some(task.clone()));
        let waker = Waker::from(Arc::new(NoWake));
        let polled = future.as_mut().poll(&mut Context::from_waker(&waker));
        self.0.current_task.replace(previous);
        match polled {
            Poll::Pending => *task.future.borrow_mut() = Some(future),
            Poll::Ready(Ok(value)) => self.resolve_promise(&task.promise, value),
            Poll::Ready(Err(thrown)) => self.settle(&task.promise, Err(thrown)),
        }
    }

    /// The operand of `await`.
    pub fn await_value(&self, value: Value) -> Await {
        Await {
            rt: self.clone(),
            value: Some(value),
            result: Rc::new(RefCell::new(None)),
        }
    }

    /// testharness.js's `promise_test`: tests run one at a time, each
    /// starting from a reaction on the one before it.
    pub(super) fn promise_test(&self, args: Vec<Value>) -> Res {
        let mut args = args.into_iter();
        let function = args.next().unwrap_or_default();
        if !matches!(function, Value::Function(_)) {
            return Err(Throw::unsupported("surface:promise_test(no function)"));
        }
        let name = args.next();
        if !matches!(args.next(), None | Some(Value::Undefined)) {
            return Err(Throw::unsupported("surface:promise_test(properties)"));
        }
        let index = self.new_test(name.as_ref())?;
        let previous = self.0.promise_tests.borrow().clone();
        let previous = previous.unwrap_or_else(|| self.promise_resolve(Value::Undefined));
        let next = self.new_promise();
        *self.0.promise_tests.borrow_mut() = Some(next.clone());
        self.add_reaction(
            &previous,
            Box::new(move |rt, settled| match settled {
                Ok(_) => {
                    let body = rt.new_promise();
                    rt.run_promise_test(index, &function, &body);
                    rt.resolve_promise(&next, Value::Host(Host::Promise(body)));
                }
                Err(thrown) => rt.settle(&next, Err(thrown)),
            }),
        );
        Ok(Value::Undefined)
    }

    fn run_promise_test(&self, index: usize, function: &Value, done: &Rc<JsPromise>) {
        let test = Value::Host(Host::Test(index));
        let returned = self.step_value(index, function, test.clone(), vec![test]);
        if !self.is_finished(index) {
            let problem = match &returned {
                Value::Host(Host::Promise(_)) => None,
                Value::Object(object) if object.own("then").is_some() => {
                    Some(Throw::unsupported("thenable:object"))
                }
                value if !self.truthy(value) => Some(Throw::error(format!(
                    "promise_test: test body must return a 'thenable' object (received {value})"
                ))),
                _ => Some(Throw::error(
                    "promise_test: test body must return a 'thenable' object (received an object with no `then` method)",
                )),
            };
            if let Some(problem) = problem {
                self.finish_thrown(index, problem);
            }
        }
        self.add_done_callback(index, done);
        // Promise.resolve(promise).catch(step_func(...)).then(() => test.done())
        let promise = self.promise_resolve(returned);
        self.add_reaction(
            &promise,
            Box::new(move |rt, settled| {
                if let Err(thrown) = settled {
                    rt.finish_thrown(index, thrown);
                }
                rt.enqueue(Box::new(move |rt| rt.done(index)));
            }),
        );
    }

    /// testharness.js's `add_test_done_callback`, resolving `promise`. After
    /// an unsupported test the queue stops: the next test would start from
    /// a state the translation cannot vouch for.
    fn add_done_callback(&self, index: usize, promise: &Rc<JsPromise>) {
        let status = {
            let mut tests = self.0.tests.borrow_mut();
            let test = &mut tests[index];
            if !test.finished {
                test.done_callbacks.push(promise.clone());
                return;
            }
            test.status.clone()
        };
        match status {
            Status::Unsupported(reason) => self.stall(&reason),
            _ => self.resolve_promise(promise, Value::Undefined),
        }
    }
}
