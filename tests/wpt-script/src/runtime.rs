//! The runtime that WPT `testharness.js` tests translated by
//! `tools/wpt_script/codegen.py` run on (RFC 0005, section 3).
//!
//! The generator compiles the JavaScript subset it accepts into Rust that
//! calls this module: JavaScript values are [`Value`]s, variables are
//! shared [`Cell`]s so closures see each other's writes, and every web
//! surface is a host function that calls the public `divui` API. Nothing
//! is interpreted from source text.
//!
//! A surface Div has no call for, or a difference between the web API and
//! `divui` that no exact translation covers (for example a frame handle
//! used as a number), stops the subtest with [`Throw::Unsupported`]: its
//! result is `UNSUPPORTED` with the reason, never a near equivalent.
//!
//! Time is virtual. Frame *n* runs at *n* × 1000/60 ms through
//! [`TestApp::advance_frame`]; before each frame, the timers due by then
//! run. A test that has not finished after 10 s of virtual time, WPT's
//! default timeout, times out.

use div::prelude::LogicalSize;
use div::testing::TestApp;
use div::{AnimationFrameHandle, Document, Element, ResizeObserver};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::rc::Rc;

mod dom;
mod promise;
mod resize;

pub use promise::{Await, JsPromise};
pub use resize::Helper;

/// The result of evaluating JavaScript: a value, or what was thrown.
pub type Res = Result<Value, Throw>;

/// A JavaScript variable, shared by every closure that captures it.
pub type Cell = Rc<RefCell<Value>>;

/// The interval between frames: a 60 Hz display's.
pub const FRAME_MS: f64 = 1000.0 / 60.0;

/// WPT's default test timeout.
pub const TIMEOUT_MS: f64 = 10_000.0;

/// The intrinsic helper the `ResizeTestHelper` globals come from.
const RESIZE_TEST_HELPER: &str = "resize-observer/resources/resizeTestHelper.js";

/// A new variable holding `value`.
pub fn cell(value: Value) -> Cell {
    Rc::new(RefCell::new(value))
}

/// The variable's value. The borrow ends before the caller continues, so
/// a closure called later may assign it.
pub fn load(cell: &Cell) -> Value {
    cell.borrow().clone()
}

/// Assign the variable.
pub fn store(cell: &Cell, value: Value) {
    *cell.borrow_mut() = value;
}

/// A `let` or `const` binding: before its declaration, the
/// `uninitialized` cell it starts as.
pub fn binding() -> Cell {
    cell(Value::Uninitialized)
}

/// A sloppy-mode function's `this`: the global object when the caller gave
/// none. A primitive `this` would be boxed, which is not translated.
pub fn sloppy_this(this: Value) -> Res {
    match this {
        Value::Undefined | Value::Null => Ok(Value::Host(Host::Window)),
        Value::Bool(_) | Value::Number(_) | Value::Str(_) => {
            Err(Throw::unsupported("sloppy-this-primitive"))
        }
        this => Ok(this),
    }
}

/// Read a `let` or `const` binding, which throws in its temporal dead zone.
pub fn read(cell: &Cell, name: &str) -> Res {
    match &*cell.borrow() {
        Value::Uninitialized => Err(Throw::error(format!(
            "ReferenceError: Cannot access '{name}' before initialization"
        ))),
        value => Ok(value.clone()),
    }
}

/// Assign a `let` binding, which throws in its temporal dead zone.
pub fn write(cell: &Cell, name: &str, value: Value) -> Result<(), Throw> {
    read(cell, name)?;
    store(cell, value);
    Ok(())
}

/// A JavaScript value.
#[derive(Clone, Default)]
pub enum Value {
    #[default]
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Str(Rc<str>),
    Object(Rc<Object>),
    Function(Rc<Function>),
    Host(Host),
    /// A `let` or `const` binding before its declaration runs (the
    /// temporal dead zone). Never a value a script can hold.
    Uninitialized,
}

/// A value that stands for something in Div.
#[derive(Clone)]
pub enum Host {
    /// The global object, `window`.
    Window,
    /// `document`.
    Document,
    /// A handle from `requestAnimationFrame`. The web's is a number; Div's
    /// is opaque, so only identity is available.
    FrameHandle(AnimationFrameHandle),
    /// A testharness `Test`, by index.
    Test(usize),
    /// An element.
    Element(Element),
    /// An element's `style` object.
    Style(Element),
    /// An interface object: `Promise`, `ResizeObserver`, `ResizeTestHelper`.
    Constructor(&'static str),
    /// A promise.
    Promise(Rc<JsPromise>),
    /// A `ResizeObserver`.
    ResizeObserver(ResizeObserver),
    /// A `ResizeTestHelper`, the intrinsic of `resizeTestHelper.js`.
    Helper(Rc<Helper>),
}

/// An ordinary object or array: properties in insertion order.
#[derive(Default)]
pub struct Object {
    array: bool,
    /// A read-only platform object's interface: reading a member it does
    /// not list, or writing any, is reported.
    record: Option<&'static str>,
    properties: RefCell<Vec<(Rc<str>, Value)>>,
}

impl Object {
    /// The own property `key`.
    fn own(&self, key: &str) -> Option<Value> {
        self.properties
            .borrow()
            .iter()
            .find(|(name, _)| &**name == key)
            .map(|(_, value)| value.clone())
    }
}

/// `Object.prototype`'s properties, which no generated object has: reading
/// one is reported, never answered with `undefined`.
const OBJECT_PROTOTYPE: &[&str] = &[
    "constructor",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "toLocaleString",
    "toString",
    "valueOf",
    "__proto__",
];

type NativeFn = dyn Fn(&Rt, Value, Vec<Value>) -> Res;

/// A function: compiled JavaScript or a host function.
pub struct Function {
    name: Rc<str>,
    call: Box<NativeFn>,
}

/// What a step threw.
#[derive(Clone, Debug)]
pub enum Throw {
    /// A JavaScript exception, including a failed assertion: the message.
    Error(String),
    /// Something the translation cannot express exactly: the reason.
    Unsupported(String),
}

impl Throw {
    pub fn error(message: impl Into<String>) -> Self {
        Throw::Error(message.into())
    }

    pub fn unsupported(reason: impl Into<String>) -> Self {
        Throw::Unsupported(reason.into())
    }
}

impl Value {
    pub fn str(text: &str) -> Value {
        Value::Str(Rc::from(text))
    }

    pub fn number(value: f64) -> Value {
        Value::Number(value)
    }

    /// A function value from a closure.
    pub fn function(name: &str, call: impl Fn(&Rt, Value, Vec<Value>) -> Res + 'static) -> Value {
        Value::Function(Rc::new(Function {
            name: Rc::from(name),
            call: Box::new(call),
        }))
    }

    /// An object with `properties`, in order.
    pub fn object(properties: Vec<(&str, Value)>) -> Value {
        Value::Object(Rc::new(Object {
            array: false,
            record: None,
            properties: RefCell::new(
                properties
                    .into_iter()
                    .map(|(key, value)| (Rc::from(key), value))
                    .collect(),
            ),
        }))
    }

    /// A read-only platform object of interface `class`.
    pub fn record(class: &'static str, properties: Vec<(&str, Value)>) -> Value {
        Value::Object(Rc::new(Object {
            array: false,
            record: Some(class),
            properties: RefCell::new(
                properties
                    .into_iter()
                    .map(|(key, value)| (Rc::from(key), value))
                    .collect(),
            ),
        }))
    }

    /// An array: an object with index properties and `length`.
    pub fn array(items: Vec<Value>) -> Value {
        let length = items.len();
        let mut properties: Vec<(Rc<str>, Value)> = items
            .into_iter()
            .enumerate()
            .map(|(index, value)| (Rc::from(index.to_string().as_str()), value))
            .collect();
        properties.push((Rc::from("length"), Value::Number(length as f64)));
        Value::Object(Rc::new(Object {
            array: true,
            record: None,
            properties: RefCell::new(properties),
        }))
    }

    fn kind(&self) -> &'static str {
        match self {
            Value::Undefined => "undefined",
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::Str(_) => "string",
            Value::Object(_) => "object",
            Value::Function(_) => "function",
            Value::Host(Host::FrameHandle(_)) => "frame handle",
            Value::Host(Host::Test(_)) => "Test",
            Value::Host(Host::Element(_)) => "element",
            Value::Host(Host::Style(_)) => "style",
            Value::Host(Host::Promise(_)) => "promise",
            Value::Host(Host::ResizeObserver(_)) => "ResizeObserver",
            Value::Host(Host::Helper(_)) => "ResizeTestHelper",
            Value::Host(Host::Constructor(name)) => name,
            Value::Host(_) => "host object",
            Value::Uninitialized => "uninitialized",
        }
    }
}

impl fmt::Display for Value {
    /// testharness.js's `format_value`, for the kinds the runtime has.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Undefined => f.write_str("undefined"),
            Value::Null => f.write_str("null"),
            Value::Bool(value) => write!(f, "{value}"),
            Value::Number(value) => f.write_str(&number_to_string(*value)),
            Value::Str(text) => write!(f, "\"{text}\""),
            Value::Object(_) => f.write_str("object \"[object Object]\""),
            Value::Function(function) => write!(f, "function \"{}\"", function.name),
            Value::Host(_) | Value::Uninitialized => f.write_str(self.kind()),
        }
    }
}

/// JavaScript's `Number::toString` for the values tests print.
/// JavaScript's `Number::toString(x, 10)` (ECMA-262 6.1.6.1.20).
///
/// The digits are the shortest that round-trip, which Rust's `{:e}` gives,
/// as JavaScript requires; this function only places them.
pub fn number_to_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value == 0.0 {
        return "0".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    let scientific = format!("{:e}", value.abs());
    let (mantissa, exponent) = scientific.split_once('e').expect("{:e} has an exponent");
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let k = digits.len() as i32;
    let n = exponent
        .parse::<i32>()
        .expect("{:e}'s exponent is an integer")
        + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let exponent = n - 1;
        let exponent_sign = if exponent < 0 { "-" } else { "+" };
        let (first, rest) = digits.split_at(1);
        let fraction = if rest.is_empty() {
            String::new()
        } else {
            format!(".{rest}")
        };
        format!("{first}{fraction}e{exponent_sign}{}", exponent.abs())
    };
    format!("{sign}{body}")
}

/// JavaScript's `StringToNumber` (ECMA-262 7.1.4.1.1) for decimal strings.
/// Hexadecimal, octal and binary literals are not translated.
fn string_to_number(text: &str) -> Result<f64, Throw> {
    // StrWhiteSpaceChar: WhiteSpace and LineTerminator.
    let space = |c: char| {
        matches!(
            c,
            '\u{9}'
                | '\u{b}'
                | '\u{c}'
                | ' '
                | '\u{a0}'
                | '\u{feff}'
                | '\n'
                | '\r'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
        )
    };
    let text = text.trim_matches(space);
    if text.is_empty() {
        return Ok(0.0);
    }
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let lower = unsigned.get(..2).map(str::to_ascii_lowercase);
    if unsigned.len() == text.len() && matches!(lower.as_deref(), Some("0x" | "0o" | "0b")) {
        return Err(Throw::unsupported("non-decimal-string-to-number"));
    }
    if unsigned == "Infinity" {
        return Ok(if text.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        });
    }
    // StrUnsignedDecimalLiteral without Infinity: digits, an optional
    // fraction, an optional exponent; at least one digit before the
    // exponent.
    let bytes = unsigned.as_bytes();
    let mut i = 0;
    let digits = |i: &mut usize| {
        let start = *i;
        while *i < bytes.len() && bytes[*i].is_ascii_digit() {
            *i += 1;
        }
        *i - start
    };
    let mut mantissa_digits = digits(&mut i);
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        mantissa_digits += digits(&mut i);
    }
    let mut valid = mantissa_digits > 0;
    if valid && i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        valid = digits(&mut i) > 0;
    }
    if !valid || i != bytes.len() {
        return Ok(f64::NAN);
    }
    // Rust's parse is correctly rounded, as StringToNumber requires.
    Ok(text.parse().expect("a validated decimal literal"))
}

/// ECMA-262 `ToInt32`, which WebIDL `long` uses (without `[EnforceRange]`).
pub fn to_int32(value: f64) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    let modulo = value.trunc().rem_euclid(4_294_967_296.0);
    if modulo >= 2_147_483_648.0 {
        (modulo - 4_294_967_296.0) as i32
    } else {
        modulo as i32
    }
}

/// A subtest's state, as testharness.js reports it.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    NotRun,
    Pass,
    Fail(String),
    Timeout,
    Unsupported(String),
}

struct Subtest {
    name: String,
    status: Status,
    finished: bool,
    cleanups: Vec<Value>,
    /// `add_test_done_callback`'s promises: the `promise_test` queue.
    done_callbacks: Vec<Rc<JsPromise>>,
}

struct Timer {
    due_ms: f64,
    order: u64,
    nesting: u32,
    callback: Value,
    this: Value,
    args: Vec<Value>,
    test: Option<usize>,
}

struct Inner {
    app: RefCell<TestApp>,
    document: Document,
    title: Option<String>,
    now_ms: std::cell::Cell<f64>,
    tests: RefCell<Vec<Subtest>>,
    timers: RefCell<Vec<Timer>>,
    timer_order: std::cell::Cell<u64>,
    /// The timer nesting level of the timer task running now (0 outside
    /// one), as HTML's timer initialization steps count it.
    timer_nesting: std::cell::Cell<u32>,
    harness_error: RefCell<Option<String>>,
    unsupported: RefCell<Option<String>>,
    /// Why the page's chain of tests stopped: the tests still waiting on it
    /// did not run.
    stopped: RefCell<Option<String>>,
    /// `get_title()`'s suffix counter for unnamed tests.
    name_counter: std::cell::Cell<u32>,
    helpers: RefCell<Vec<String>>,
    microtasks: RefCell<VecDeque<promise::Microtask>>,
    in_checkpoint: std::cell::Cell<bool>,
    rejections: RefCell<Vec<Rc<JsPromise>>>,
    current_task: RefCell<Option<Rc<promise::Task>>>,
    promise_tests: RefCell<Option<Rc<JsPromise>>>,
    /// The web keeps an observer with targets alive however few references
    /// a script holds; `divui` stops one when its last handle drops.
    observers: RefCell<Vec<ResizeObserver>>,
}

/// One page's runtime: its document, its harness and its clock.
#[derive(Clone)]
pub struct Rt(Rc<Inner>);

/// The outcome of a page: each subtest's status, in declaration order,
/// and the harness's error if the page threw outside a test.
#[derive(Debug)]
pub struct PageResult {
    pub subtests: Vec<(String, Status)>,
    pub harness_error: Option<String>,
    /// Why the page could not be built or run exactly, if it could not:
    /// every subtest it would have declared is unsupported.
    pub unsupported: Option<String>,
    /// Why the page's chain of tests stopped, if it did: an unsupported
    /// operation no step reported. The tests waiting on it did not run.
    pub stopped: Option<String>,
}

impl Rt {
    /// A page titled `title`, in an 800×600 viewport at scale 1 (the size
    /// the Chromium recordings use).
    pub fn new(title: Option<&str>) -> Rt {
        let app = TestApp::new(LogicalSize::new(800.0, 600.0));
        let document = app.document().clone();
        Rt(Rc::new(Inner {
            app: RefCell::new(app),
            document,
            title: title.map(str::to_owned),
            now_ms: std::cell::Cell::new(0.0),
            tests: RefCell::new(Vec::new()),
            timers: RefCell::new(Vec::new()),
            timer_order: std::cell::Cell::new(0),
            timer_nesting: std::cell::Cell::new(0),
            harness_error: RefCell::new(None),
            unsupported: RefCell::new(None),
            stopped: RefCell::new(None),
            name_counter: std::cell::Cell::new(0),
            helpers: RefCell::new(Vec::new()),
            microtasks: RefCell::new(VecDeque::new()),
            in_checkpoint: std::cell::Cell::new(false),
            rejections: RefCell::new(Vec::new()),
            current_task: RefCell::new(None),
            promise_tests: RefCell::new(None),
            observers: RefCell::new(Vec::new()),
        }))
    }

    pub fn document(&self) -> &Document {
        &self.0.document
    }

    /// Run a page: its markup and script, then frames until every test
    /// has finished or the timeout passes.
    pub fn run_page(&self, page: impl FnOnce(&Rt)) -> PageResult {
        page(self);
        while self.has_pending_tests() && self.0.now_ms.get() < TIMEOUT_MS {
            self.frame();
        }
        // The observers' callbacks hold this runtime.
        self.0.observers.borrow_mut().clear();
        let stopped = self.0.stopped.borrow().clone();
        let mut tests = self.0.tests.borrow_mut();
        for test in tests.iter_mut().filter(|test| !test.finished) {
            test.status = if stopped.is_some() {
                Status::NotRun
            } else {
                Timeout
            };
            test.finished = true;
        }
        PageResult {
            subtests: tests
                .iter()
                .map(|test| (test.name.clone(), test.status.clone()))
                .collect(),
            harness_error: self.0.harness_error.borrow().clone(),
            unsupported: self.0.unsupported.borrow().clone(),
            stopped,
        }
    }

    /// The page loaded the intrinsic helper at `path`.
    pub fn load_helper(&self, path: &str) {
        self.0.helpers.borrow_mut().push(path.to_owned());
    }

    fn has_helper(&self, path: &str) -> bool {
        self.0.helpers.borrow().iter().any(|helper| helper == path)
    }

    /// The page's chain of tests cannot continue exactly, for `reason`.
    fn stall(&self, reason: &str) {
        self.0
            .stopped
            .borrow_mut()
            .get_or_insert_with(|| reason.to_owned());
    }

    /// A rejection nothing handled: testharness.js reports a harness error.
    /// An unsupported operation is not a rejection the page could see: it
    /// stops the chain the promise belonged to.
    fn unhandled_rejection(&self, thrown: Throw) {
        match thrown {
            Throw::Error(message) => {
                self.0
                    .harness_error
                    .borrow_mut()
                    .get_or_insert(format!("Unhandled rejection: {message}"));
            }
            Throw::Unsupported(reason) => self.stall(&reason),
        }
    }

    /// `requestAnimationFrame` for a native callback.
    fn native_frame(
        &self,
        callback: impl FnOnce(&Rt) -> Result<(), Throw> + 'static,
    ) -> AnimationFrameHandle {
        let host = self.clone();
        self.document().request_animation_frame(move |_| {
            if let Err(thrown) = callback(&host) {
                host.uncaught(thrown);
            }
            host.checkpoint();
        })
    }

    /// The page cannot be built or run exactly, for `reason`: the subtests
    /// it declares are unsupported.
    pub fn unsupported_page(&self, reason: &str) {
        self.0
            .unsupported
            .borrow_mut()
            .get_or_insert_with(|| reason.to_owned());
    }

    fn has_pending_tests(&self) -> bool {
        self.0.tests.borrow().iter().any(|test| !test.finished)
    }

    /// One frame: the timers due by its time, then Div's rendering steps.
    fn frame(&self) {
        let next = self.0.now_ms.get() + FRAME_MS;
        loop {
            let due = {
                let mut timers = self.0.timers.borrow_mut();
                let index = timers
                    .iter()
                    .enumerate()
                    .filter(|(_, timer)| timer.due_ms <= next)
                    .min_by(|(_, a), (_, b)| {
                        a.due_ms.total_cmp(&b.due_ms).then(a.order.cmp(&b.order))
                    })
                    .map(|(index, _)| index);
                index.map(|index| timers.remove(index))
            };
            let Some(timer) = due else { break };
            self.0.now_ms.set(self.0.now_ms.get().max(timer.due_ms));
            self.0.timer_nesting.set(timer.nesting);
            match timer.test {
                Some(test) => self.step(test, &timer.callback, timer.this, timer.args),
                None => {
                    if let Err(thrown) = self.call(&timer.callback, timer.this, timer.args) {
                        self.uncaught(thrown);
                    }
                }
            }
            self.checkpoint();
            self.0.timer_nesting.set(0);
        }
        self.0.now_ms.set(next);
        self.0.app.borrow_mut().advance_frame(FRAME_MS);
    }

    /// An exception no step caught: the harness reports an error, as
    /// testharness.js does. An unsupported operation outside a step stops
    /// every test still running, since none of them can finish exactly.
    pub fn uncaught(&self, thrown: Throw) {
        match thrown {
            Throw::Error(message) => {
                self.0.harness_error.borrow_mut().get_or_insert(message);
            }
            Throw::Unsupported(reason) => {
                self.unsupported_page(&reason);
                for test in self.0.tests.borrow_mut().iter_mut() {
                    if !test.finished {
                        test.status = Status::Unsupported(reason.clone());
                        test.finished = true;
                    }
                }
            }
        }
    }

    // ----- testharness.js -----

    fn new_test(&self, name: Option<&Value>) -> Result<usize, Throw> {
        let name = match name {
            None | Some(Value::Undefined) => match &self.0.title {
                // testharness.js's `next_default_test_name`.
                Some(title) => {
                    let counter = self.0.name_counter.replace(self.0.name_counter.get() + 1);
                    if counter > 0 {
                        format!("{title} {counter}")
                    } else {
                        title.clone()
                    }
                }
                None => return Err(Throw::unsupported("default-test-name")),
            },
            Some(Value::Str(text)) => text.to_string(),
            Some(other) => self.to_string(other)?,
        };
        let mut tests = self.0.tests.borrow_mut();
        tests.push(Subtest {
            name,
            status: Status::NotRun,
            finished: false,
            cleanups: Vec::new(),
            done_callbacks: Vec::new(),
        });
        Ok(tests.len() - 1)
    }

    /// Run `function` as a step of test `index`: an exception fails the
    /// test; nothing runs once the test has finished.
    pub fn step(&self, index: usize, function: &Value, this: Value, args: Vec<Value>) {
        self.step_value(index, function, this, args);
    }

    fn step_value(&self, index: usize, function: &Value, this: Value, args: Vec<Value>) -> Value {
        if self.0.tests.borrow()[index].finished {
            return Value::Undefined;
        }
        match self.call(function, this, args) {
            Ok(value) => value,
            Err(thrown) => {
                self.finish_thrown(index, thrown);
                Value::Undefined
            }
        }
    }

    fn finish(&self, index: usize, status: Status) {
        let unsupported = match &status {
            Status::Unsupported(reason) => Some(reason.clone()),
            _ => None,
        };
        let (cleanups, callbacks) = {
            let mut tests = self.0.tests.borrow_mut();
            let test = &mut tests[index];
            if test.finished {
                return;
            }
            test.status = status;
            test.finished = true;
            (
                std::mem::take(&mut test.cleanups),
                std::mem::take(&mut test.done_callbacks),
            )
        };
        for cleanup in cleanups {
            if let Err(thrown) = self.call(&cleanup, Value::Undefined, Vec::new()) {
                self.uncaught(thrown);
            }
        }
        for callback in callbacks {
            // The `promise_test` queue stops after an unsupported test: the
            // next would start from a state the translation cannot vouch for.
            match &unsupported {
                Some(reason) => self.stall(reason),
                None => self.resolve_promise(&callback, Value::Undefined),
            }
        }
    }

    /// Finish test `index` with what a step threw.
    fn finish_thrown(&self, index: usize, thrown: Throw) {
        let status = match thrown {
            Throw::Error(message) => Status::Fail(message),
            Throw::Unsupported(reason) => Status::Unsupported(reason),
        };
        self.finish(index, status);
    }

    fn is_finished(&self, index: usize) -> bool {
        self.0.tests.borrow()[index].finished
    }

    fn add_cleanup(&self, index: usize, cleanup: Value) {
        self.0.tests.borrow_mut()[index].cleanups.push(cleanup);
    }

    fn done(&self, index: usize) {
        self.finish(index, Status::Pass);
    }

    fn test_method(&self, index: usize, name: &str) -> Res {
        let rt = self.clone();
        let method = match name {
            "done" => Value::function("done", move |_, _, _| {
                rt.done(index);
                Ok(Value::Undefined)
            }),
            "step" => Value::function("step", move |rt, _, args| {
                let mut args = args.into_iter();
                let function = args.next().unwrap_or_default();
                let this = args.next().unwrap_or_default();
                Ok(rt.step_value(index, &function, this, args.collect()))
            }),
            "step_func" | "step_func_done" => {
                let done = name == "step_func_done";
                Value::function(name, move |_, _, args| {
                    let function = args.into_iter().next().unwrap_or_default();
                    let rt = rt.clone();
                    Ok(Value::function("step", move |_, this, args| {
                        let value = if matches!(function, Value::Undefined) {
                            Value::Undefined
                        } else {
                            rt.step_value(index, &function, this, args)
                        };
                        if done {
                            rt.done(index);
                        }
                        Ok(value)
                    }))
                })
            }
            "unreached_func" => Value::function("unreached_func", move |_, _, args| {
                let description = args.into_iter().next().unwrap_or_default();
                let rt = rt.clone();
                Ok(Value::function("unreached", move |_, _, _| {
                    let description = rt.to_string(&description).unwrap_or_default();
                    rt.finish(
                        index,
                        Status::Fail(format!(
                            "assert_unreached: {description} Reached unreachable code"
                        )),
                    );
                    Ok(Value::Undefined)
                }))
            }),
            "add_cleanup" => Value::function("add_cleanup", move |_, _, args| {
                rt.add_cleanup(index, args.into_iter().next().unwrap_or_default());
                Ok(Value::Undefined)
            }),
            "step_timeout" => Value::function("step_timeout", move |_, _, args| {
                // testharness.js: setTimeout(this.step_func(...), timeout),
                // calling `f` with the test as `this` and the other
                // arguments.
                let mut args = args.into_iter();
                let function = args.next().unwrap_or_default();
                let delay = match args.next() {
                    Some(value) => to_int32(rt.to_number(&value)?),
                    None => 0,
                };
                let id = rt.set_timer(
                    function,
                    Value::Host(Host::Test(index)),
                    args.collect(),
                    delay,
                    Some(index),
                )?;
                Ok(Value::Number(id as f64))
            }),
            "name" => Value::str(&self.0.tests.borrow()[index].name),
            other => return Err(Throw::unsupported(format!("surface:Test.{other}"))),
        };
        Ok(method)
    }

    /// `setTimeout`'s timer initialization steps, for a delay already
    /// converted to WebIDL `long`. Returns the timer's handle.
    fn set_timer(
        &self,
        callback: Value,
        this: Value,
        args: Vec<Value>,
        delay_ms: i32,
        test: Option<usize>,
    ) -> Result<u64, Throw> {
        let nesting = self.0.timer_nesting.get() + 1;
        // HTML raises a deeply nested short timeout to 4 ms; browsers
        // differ on the level, so a timer that could be clamped is not
        // translated.
        if nesting > 4 && delay_ms < 4 {
            return Err(Throw::unsupported("timer-nesting-clamp"));
        }
        let order = self.0.timer_order.get() + 1;
        self.0.timer_order.set(order);
        self.0.timers.borrow_mut().push(Timer {
            due_ms: self.0.now_ms.get() + f64::from(delay_ms.max(0)),
            order,
            nesting,
            callback,
            this,
            args,
            test,
        });
        Ok(order)
    }

    /// `clearTimeout`.
    fn clear_timer(&self, id: u64) {
        self.0.timers.borrow_mut().retain(|timer| timer.order != id);
    }

    // ----- globals -----

    /// The global `name`. A name the runtime has no Div call for stops the
    /// subtest.
    pub fn global(&self, name: &str) -> Res {
        let rt = self.clone();
        Ok(match name {
            "window" | "self" | "globalThis" => Value::Host(Host::Window),
            "document" => Value::Host(Host::Document),
            "undefined" => Value::Undefined,
            "NaN" => Value::Number(f64::NAN),
            "Infinity" => Value::Number(f64::INFINITY),
            "test" => Value::function("test", move |rt, _, args| {
                let mut args = args.into_iter();
                let function = args.next().unwrap_or_default();
                let index = rt.new_test(args.next().as_ref())?;
                let test = Value::Host(Host::Test(index));
                rt.step(index, &function, test.clone(), vec![test]);
                rt.done(index);
                Ok(Value::Undefined)
            }),
            "async_test" => Value::function("async_test", move |rt, _, args| {
                let mut args = args.into_iter();
                let first = args.next().unwrap_or_default();
                let (function, name) = match first {
                    Value::Function(_) => (Some(first), args.next()),
                    name => (None, Some(name)),
                };
                let index = rt.new_test(name.as_ref())?;
                let test = Value::Host(Host::Test(index));
                if let Some(function) = function {
                    rt.step(index, &function, test.clone(), vec![test.clone()]);
                }
                Ok(test)
            }),
            "promise_test" => Value::function("promise_test", |rt, _, args| rt.promise_test(args)),
            "Promise" => Value::Host(Host::Constructor("Promise")),
            "ResizeObserver" => Value::Host(Host::Constructor("ResizeObserver")),
            "ResizeTestHelper" if self.has_helper(RESIZE_TEST_HELPER) => {
                Value::Host(Host::Constructor("ResizeTestHelper"))
            }
            "createAndAppendElement" if self.has_helper(RESIZE_TEST_HELPER) => {
                Value::function("createAndAppendElement", |rt, _, args| {
                    rt.create_and_append_element(args)
                })
            }
            "setup" => Value::function("setup", |_, _, args| {
                // Only an empty or function-only setup is exact here; options
                // change the harness and are reported.
                match args.first() {
                    None | Some(Value::Function(_)) => Ok(Value::Undefined),
                    Some(_) => Err(Throw::unsupported("surface:setup(options)")),
                }
            }),
            "requestAnimationFrame" => {
                Value::function("requestAnimationFrame", move |_, _, args| {
                    let callback = args.into_iter().next().unwrap_or_default();
                    if !matches!(callback, Value::Function(_)) {
                        return Err(Throw::error(
                        "TypeError: Failed to execute 'requestAnimationFrame' on 'Window': The callback provided as parameter 1 is not a function.",
                    ));
                    }
                    let host = rt.clone();
                    let handle = rt.document().request_animation_frame(move |time_ms| {
                        if let Err(thrown) =
                            host.call(&callback, Value::Undefined, vec![Value::Number(time_ms)])
                        {
                            host.uncaught(thrown);
                        }
                        host.checkpoint();
                    });
                    Ok(Value::Host(Host::FrameHandle(handle)))
                })
            }
            "cancelAnimationFrame" => Value::function("cancelAnimationFrame", move |_, _, args| {
                match args.into_iter().next().unwrap_or_default() {
                    Value::Host(Host::FrameHandle(handle)) => {
                        rt.document().cancel_animation_frame(handle);
                        Ok(Value::Undefined)
                    }
                    _ => Err(Throw::unsupported("numeric-frame-handle")),
                }
            }),
            name if name.starts_with("assert_") => assertion(name)?,
            other => return Err(Throw::unsupported(format!("surface:{other}"))),
        })
    }

    /// `typeof name` for a name that may not exist.
    pub fn type_of_global(&self, name: &str) -> Res {
        match self.global(name) {
            Ok(value) => self.type_of(&value),
            Err(Throw::Unsupported(reason)) => Err(Throw::Unsupported(reason)),
            Err(error) => Err(error),
        }
    }

    // ----- operations -----

    pub fn call(&self, function: &Value, this: Value, args: Vec<Value>) -> Res {
        match function {
            Value::Function(function) => (function.call)(self, this, args),
            other => Err(Throw::error(format!(
                "TypeError: {} is not a function",
                other.kind()
            ))),
        }
    }

    pub fn call_method(&self, object: &Value, name: &str, args: Vec<Value>) -> Res {
        let function = self.get(object, name)?;
        self.call(&function, object.clone(), args)
    }

    /// A property key from a computed member expression.
    pub fn key(&self, value: &Value) -> Result<String, Throw> {
        self.to_string(value)
    }

    pub fn get(&self, object: &Value, key: &str) -> Res {
        match object {
            Value::Object(object) => match object.own(key) {
                Some(value) => Ok(value),
                None if object.record.is_some() => Err(Throw::unsupported(format!(
                    "surface:{}.{key}",
                    object.record.unwrap_or_default()
                ))),
                None if OBJECT_PROTOTYPE.contains(&key) => Err(Throw::unsupported(format!(
                    "builtin:Object.prototype.{key}"
                ))),
                None if object.array && key.parse::<usize>().is_err() => {
                    Err(Throw::unsupported(format!("builtin:Array.prototype.{key}")))
                }
                None => Ok(Value::Undefined),
            },
            Value::Host(Host::Window) => self.global(key),
            Value::Host(Host::Test(index)) => self.test_method(*index, key),
            Value::Host(Host::Document) => self.document_member(key),
            Value::Host(Host::Element(element)) => self.element_member(element, key),
            Value::Host(Host::Promise(promise)) => self.promise_member(promise, key),
            Value::Host(Host::Constructor("Promise")) => self.promise_static(key),
            Value::Host(Host::ResizeObserver(observer)) => {
                self.resize_observer_member(observer, key)
            }
            Value::Host(Host::Helper(helper)) => self.helper_member(helper, key),
            Value::Str(text) if key == "length" => {
                Ok(Value::Number(text.encode_utf16().count() as f64))
            }
            Value::Undefined | Value::Null => Err(Throw::error(format!(
                "TypeError: Cannot read properties of {} (reading '{key}')",
                object.kind()
            ))),
            other => Err(Throw::unsupported(format!(
                "surface:{}.{key}",
                other.kind()
            ))),
        }
    }

    pub fn set(&self, object: &Value, key: &str, value: Value) -> Result<(), Throw> {
        match object {
            Value::Object(object) if object.array => {
                Err(Throw::unsupported(format!("builtin:array-set:{key}")))
            }
            Value::Object(object) if object.record.is_some() => Err(Throw::unsupported(format!(
                "surface:{}.{key}=",
                object.record.unwrap_or_default()
            ))),
            Value::Host(Host::Style(element)) => self.set_style(element, key, &value),
            Value::Object(object) => {
                let mut properties = object.properties.borrow_mut();
                match properties.iter_mut().find(|(name, _)| &**name == key) {
                    Some((_, slot)) => *slot = value,
                    None => properties.push((Rc::from(key), value)),
                }
                Ok(())
            }
            Value::Undefined | Value::Null => Err(Throw::error(format!(
                "TypeError: Cannot set properties of {} (setting '{key}')",
                object.kind()
            ))),
            other => Err(Throw::unsupported(format!(
                "surface:{}.{key}=",
                other.kind()
            ))),
        }
    }

    pub fn construct(&self, callee: &Value, args: Vec<Value>) -> Res {
        match callee {
            Value::Host(Host::Constructor("Promise")) => self.construct_promise(args),
            Value::Host(Host::Constructor("ResizeObserver")) => {
                self.construct_resize_observer(args)
            }
            Value::Host(Host::Constructor("ResizeTestHelper")) => self.construct_helper(args),
            other => Err(Throw::unsupported(format!("surface:new {}", other.kind()))),
        }
    }

    pub fn truthy(&self, value: &Value) -> bool {
        match value {
            Value::Undefined | Value::Null => false,
            Value::Bool(value) => *value,
            Value::Number(value) => *value != 0.0 && !value.is_nan(),
            Value::Str(text) => !text.is_empty(),
            _ => true,
        }
    }

    pub fn type_of(&self, value: &Value) -> Res {
        Ok(Value::str(match value {
            Value::Undefined => "undefined",
            Value::Null | Value::Object(_) => "object",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::Str(_) => "string",
            Value::Function(_) | Value::Host(Host::Constructor(_)) => "function",
            Value::Host(Host::FrameHandle(_)) => {
                return Err(Throw::unsupported("numeric-frame-handle"))
            }
            Value::Host(_) => "object",
            Value::Uninitialized => return Err(Throw::unsupported("uninitialized-value")),
        }))
    }

    pub fn to_number(&self, value: &Value) -> Result<f64, Throw> {
        match value {
            Value::Undefined => Ok(f64::NAN),
            Value::Null => Ok(0.0),
            Value::Bool(value) => Ok(if *value { 1.0 } else { 0.0 }),
            Value::Number(value) => Ok(*value),
            Value::Str(text) => string_to_number(text),
            Value::Host(Host::FrameHandle(_)) => Err(Throw::unsupported("numeric-frame-handle")),
            other => Err(Throw::unsupported(format!("to-number:{}", other.kind()))),
        }
    }

    pub fn to_string(&self, value: &Value) -> Result<String, Throw> {
        match value {
            Value::Undefined => Ok("undefined".into()),
            Value::Null => Ok("null".into()),
            Value::Bool(value) => Ok(value.to_string()),
            Value::Number(value) => Ok(number_to_string(*value)),
            Value::Str(text) => Ok(text.to_string()),
            // ToPrimitive (an array's join, an object's own toString) is
            // not translated.
            other => Err(Throw::unsupported(format!("to-string:{}", other.kind()))),
        }
    }

    pub fn negate(&self, value: &Value) -> Res {
        Ok(Value::Number(-self.to_number(value)?))
    }

    pub fn not(&self, value: &Value) -> Value {
        Value::Bool(!self.truthy(value))
    }

    /// A binary operator.
    pub fn binary(&self, op: &str, a: &Value, b: &Value) -> Res {
        Ok(match op {
            "===" => Value::Bool(self.strict_equals(a, b)?),
            "!==" => Value::Bool(!self.strict_equals(a, b)?),
            "==" => Value::Bool(self.loose_equals(a, b)?),
            "!=" => Value::Bool(!self.loose_equals(a, b)?),
            "+" => match (a, b) {
                (Value::Str(_), _) | (_, Value::Str(_)) => {
                    Value::str(&(self.to_string(a)? + &self.to_string(b)?))
                }
                _ => Value::Number(self.to_number(a)? + self.to_number(b)?),
            },
            "-" => Value::Number(self.to_number(a)? - self.to_number(b)?),
            "*" => Value::Number(self.to_number(a)? * self.to_number(b)?),
            "/" => Value::Number(self.to_number(a)? / self.to_number(b)?),
            "%" => Value::Number(self.to_number(a)? % self.to_number(b)?),
            "<" | ">" | "<=" | ">=" => {
                let ordering = match (a, b) {
                    (Value::Str(x), Value::Str(y)) => Some(x.encode_utf16().cmp(y.encode_utf16())),
                    _ => self.to_number(a)?.partial_cmp(&self.to_number(b)?),
                };
                Value::Bool(match ordering {
                    None => false,
                    Some(ordering) => match op {
                        "<" => ordering.is_lt(),
                        ">" => ordering.is_gt(),
                        "<=" => ordering.is_le(),
                        _ => ordering.is_ge(),
                    },
                })
            }
            other => return Err(Throw::unsupported(format!("syntax:operator {other}"))),
        })
    }

    pub fn strict_equals(&self, a: &Value, b: &Value) -> Result<bool, Throw> {
        Ok(match (a, b) {
            (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => true,
            (Value::Bool(x), Value::Bool(y)) => x == y,
            (Value::Number(x), Value::Number(y)) => x == y,
            (Value::Str(x), Value::Str(y)) => x == y,
            (Value::Object(x), Value::Object(y)) => Rc::ptr_eq(x, y),
            (Value::Function(x), Value::Function(y)) => Rc::ptr_eq(x, y),
            (Value::Host(Host::FrameHandle(x)), Value::Host(Host::FrameHandle(y))) => x == y,
            (Value::Host(Host::FrameHandle(_)), _) | (_, Value::Host(Host::FrameHandle(_))) => {
                return Err(Throw::unsupported("numeric-frame-handle"))
            }
            (Value::Host(x), Value::Host(y)) => same_host(x, y)?,
            _ => false,
        })
    }

    fn loose_equals(&self, a: &Value, b: &Value) -> Result<bool, Throw> {
        match (a, b) {
            (Value::Undefined | Value::Null, Value::Undefined | Value::Null) => Ok(true),
            (Value::Undefined | Value::Null, _) | (_, Value::Undefined | Value::Null) => Ok(false),
            // An object against a primitive goes through ToPrimitive: not
            // translated.
            (Value::Number(_) | Value::Str(_) | Value::Bool(_), b)
                if !matches!(b, Value::Number(_) | Value::Str(_) | Value::Bool(_)) =>
            {
                Err(Throw::unsupported("loose-equality:object"))
            }
            (a, Value::Number(_) | Value::Str(_) | Value::Bool(_))
                if !matches!(a, Value::Number(_) | Value::Str(_) | Value::Bool(_)) =>
            {
                Err(Throw::unsupported("loose-equality:object"))
            }
            (
                Value::Number(_) | Value::Str(_) | Value::Bool(_),
                Value::Number(_) | Value::Str(_) | Value::Bool(_),
            ) if std::mem::discriminant(a) != std::mem::discriminant(b) => {
                Ok(self.to_number(a)? == self.to_number(b)?)
            }
            _ => self.strict_equals(a, b),
        }
    }

    /// An operation the translation cannot express exactly.
    pub fn unsupported(&self, reason: &str) -> Res {
        Err(Throw::unsupported(reason.to_owned()))
    }

    /// The items of an array, for `for...of`. Other iterables are not
    /// translated.
    pub fn iterate(&self, value: &Value) -> Result<Vec<Value>, Throw> {
        let Value::Object(object) = value else {
            return Err(Throw::unsupported(format!("iterate:{}", value.kind())));
        };
        let length = object
            .properties
            .borrow()
            .iter()
            .find(|(name, _)| &**name == "length")
            .map(|(_, value)| value.clone());
        let Some(Value::Number(length)) = length else {
            return Err(Throw::unsupported("iterate:object"));
        };
        (0..length as usize)
            .map(|index| self.get(value, &index.to_string()))
            .collect()
    }

    // ----- markup -----

    /// Create `<tag>` with `attributes` and append it to `parent` (the
    /// body when None): the page's static markup.
    pub fn element(
        &self,
        tag: &str,
        attributes: &[(&str, &str)],
        parent: Option<&Element>,
    ) -> Result<Element, Throw> {
        let element = Element::create(self.document(), tag)
            .map_err(|error| Throw::unsupported(format!("markup:{tag} ({error:?})")))?;
        for (name, value) in attributes {
            element
                .set_attribute(name, value)
                .map_err(|error| Throw::unsupported(format!("markup:{name}= ({error:?})")))?;
        }
        let body = self.document().body();
        parent
            .unwrap_or(&body)
            .append_child(&element)
            .map_err(|error| Throw::unsupported(format!("markup:append ({error:?})")))?;
        Ok(element)
    }

    /// Append the text `text` to `parent` (the body when None).
    pub fn text(&self, text: &str, parent: Option<&Element>) -> Result<(), Throw> {
        let body = self.document().body();
        parent
            .unwrap_or(&body)
            .create_text_child(text)
            .map_err(|error| Throw::unsupported(format!("markup:text ({error:?})")))?;
        Ok(())
    }
}

use Status::Timeout;

/// SameValue, which testharness.js's `assert_equals` uses.
fn same_value(a: &Value, b: &Value) -> Result<bool, Throw> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => Ok(if x.is_nan() && y.is_nan() {
            true
        } else {
            x == y && x.is_sign_negative() == y.is_sign_negative()
        }),
        _ => {
            // Strict equality for everything else; a frame handle compared
            // with anything but a frame handle is the web's number.
            match (a, b) {
                (Value::Host(Host::FrameHandle(x)), Value::Host(Host::FrameHandle(y))) => {
                    Ok(x == y)
                }
                (Value::Host(Host::FrameHandle(_)), _) | (_, Value::Host(Host::FrameHandle(_))) => {
                    Err(Throw::unsupported("numeric-frame-handle"))
                }
                (Value::Host(x), Value::Host(y)) => same_host(x, y),
                _ => {
                    Ok(std::mem::discriminant(a) == std::mem::discriminant(b)
                        && equal_same_kind(a, b))
                }
            }
        }
    }
}

fn equal_same_kind(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Object(x), Value::Object(y)) => Rc::ptr_eq(x, y),
        (Value::Function(x), Value::Function(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}

/// Identity of two host objects (frame handles are compared before this).
fn same_host(a: &Host, b: &Host) -> Result<bool, Throw> {
    Ok(match (a, b) {
        (Host::Window, Host::Window) | (Host::Document, Host::Document) => true,
        (Host::Test(x), Host::Test(y)) => x == y,
        (Host::Element(x), Host::Element(y)) | (Host::Style(x), Host::Style(y)) => {
            same_element(x, y)?
        }
        (Host::Constructor(x), Host::Constructor(y)) => x == y,
        (Host::Promise(x), Host::Promise(y)) => Rc::ptr_eq(x, y),
        (Host::ResizeObserver(x), Host::ResizeObserver(y)) => x == y,
        (Host::Helper(x), Host::Helper(y)) => Rc::ptr_eq(x, y),
        _ => false,
    })
}

/// Node identity, as `===` compares elements.
fn same_element(a: &Element, b: &Element) -> Result<bool, Throw> {
    a.is_same_node(b)
        .map_err(|error| Throw::unsupported(format!("element-identity ({error:?})")))
}

fn description(args: &[Value], index: usize) -> String {
    match args.get(index) {
        Some(Value::Str(text)) => text.to_string(),
        _ => String::new(),
    }
}

fn failure(name: &str, description: &str, message: String) -> Throw {
    if description.is_empty() {
        Throw::error(format!("{name}: {message}"))
    } else {
        Throw::error(format!("{name}: {description} {message}"))
    }
}

fn number_argument(name: &str, value: &Value, role: &str) -> Result<f64, Throw> {
    match value {
        Value::Number(value) => Ok(*value),
        Value::Host(Host::FrameHandle(_)) => Err(Throw::unsupported("numeric-frame-handle")),
        other => Err(Throw::error(format!(
            "{name}: {role} {} is not a number",
            other
        ))),
    }
}

/// testharness.js's assertions, with its semantics and tolerances.
fn assertion(name: &str) -> Res {
    let owned = name.to_owned();
    let function = move |_: &Rt, _: Value, args: Vec<Value>| -> Res {
        let name = owned.as_str();
        let arg = |index: usize| args.get(index).cloned().unwrap_or_default();
        match name {
            "assert_true" | "assert_false" => {
                let expected = name == "assert_true";
                let actual = arg(0);
                if !matches!(actual, Value::Bool(value) if value == expected) {
                    return Err(failure(
                        name,
                        &description(&args, 1),
                        format!("expected {expected} got {actual}"),
                    ));
                }
            }
            "assert_equals" => {
                let (actual, expected) = (arg(0), arg(1));
                if !same_value(&actual, &expected)? {
                    return Err(failure(
                        name,
                        &description(&args, 2),
                        format!("expected {expected} but got {actual}"),
                    ));
                }
            }
            "assert_not_equals" => {
                let (actual, expected) = (arg(0), arg(1));
                if same_value(&actual, &expected)? {
                    return Err(failure(
                        name,
                        &description(&args, 2),
                        format!("got disallowed value {actual}"),
                    ));
                }
            }
            "assert_greater_than"
            | "assert_less_than"
            | "assert_greater_than_equal"
            | "assert_less_than_equal" => {
                let actual = number_argument(name, &arg(0), "actual")?;
                let expected = number_argument(name, &arg(1), "expected")?;
                let (holds, relation) = match name {
                    "assert_greater_than" => (actual > expected, "a number greater than"),
                    "assert_less_than" => (actual < expected, "a number less than"),
                    "assert_greater_than_equal" => {
                        (actual >= expected, "a number greater than or equal to")
                    }
                    _ => (actual <= expected, "a number less than or equal to"),
                };
                if !holds {
                    return Err(failure(
                        name,
                        &description(&args, 2),
                        format!(
                            "expected {relation} {} but got {}",
                            number_to_string(expected),
                            number_to_string(actual)
                        ),
                    ));
                }
            }
            "assert_approx_equals" => {
                let actual = number_argument(name, &arg(0), "actual")?;
                let expected = number_argument(name, &arg(1), "expected")?;
                let epsilon = number_argument(name, &arg(2), "epsilon")?;
                if (actual - expected).abs() > epsilon {
                    return Err(failure(
                        name,
                        &description(&args, 3),
                        format!(
                            "expected {} +/- {} but got {}",
                            number_to_string(expected),
                            number_to_string(epsilon),
                            number_to_string(actual)
                        ),
                    ));
                }
            }
            "assert_own_property" => {
                let key = match arg(1) {
                    Value::Str(key) => key,
                    other => {
                        return Err(Throw::unsupported(format!(
                            "surface:assert_own_property({})",
                            other.kind()
                        )))
                    }
                };
                let found = match arg(0) {
                    Value::Object(object) => object.own(&key).is_some(),
                    // The interfaces the runtime exposes on the global.
                    Value::Host(Host::Window) if &*key == "ResizeObserver" => true,
                    other => {
                        return Err(Throw::unsupported(format!(
                            "surface:assert_own_property({}, {key})",
                            other.kind()
                        )))
                    }
                };
                if !found {
                    return Err(failure(
                        name,
                        &description(&args, 2),
                        format!("expected property \"{key}\" missing"),
                    ));
                }
            }
            "assert_unreached" => {
                return Err(failure(
                    name,
                    &description(&args, 0),
                    "Reached unreachable code".into(),
                ));
            }
            other => return Err(Throw::unsupported(format!("surface:{other}"))),
        }
        Ok(Value::Undefined)
    };
    Ok(Value::function(name, function))
}

/// The expected outcome of a subtest, from the ledger.
#[derive(Clone, Copy, Debug)]
pub enum Expected {
    Pass,
    Fail,
    Timeout,
    NotRun,
    Unsupported,
}

/// Check a page's results against the ledger's expectations: a regression
/// fails, and so does a pass the ledger does not record yet. With
/// `DIV_WPT_SCRIPT_RECORD` set to a directory, write the results there
/// instead (as `<page>.json`), for `ledger.py` to record.
pub fn expect(
    page: &str,
    result: &PageResult,
    harness_error: Option<&str>,
    stopped: Option<&str>,
    expected: &[(&str, Expected)],
) {
    if let Some(directory) = std::env::var_os("DIV_WPT_SCRIPT_RECORD") {
        record(std::path::Path::new(&directory), page, result);
        return;
    }
    let mut problems = Vec::new();
    if result.harness_error.as_deref() != harness_error {
        problems.push(format!(
            "harness error: expected {harness_error:?}, got {:?}",
            result.harness_error
        ));
    }
    // A chain that stops elsewhere, or for another reason, is a different
    // result even when the same subtests are left unrun.
    if result.stopped.as_deref() != stopped {
        problems.push(format!(
            "chain stopped: expected {stopped:?}, got {:?}",
            result.stopped
        ));
    }
    for (name, expected) in expected {
        let actual = result
            .subtests
            .iter()
            .find(|(subtest, _)| subtest == name)
            .map(|(_, status)| status.clone())
            .unwrap_or_else(|| match &result.unsupported {
                Some(reason) => Status::Unsupported(reason.clone()),
                None => Status::NotRun,
            });
        let matches = matches!(
            (expected, &actual),
            (Expected::Pass, Status::Pass)
                | (Expected::Fail, Status::Fail(_))
                | (Expected::Timeout, Status::Timeout)
                | (Expected::NotRun, Status::NotRun)
                | (Expected::Unsupported, Status::Unsupported(_))
        );
        if !matches {
            problems.push(format!("{name}: expected {expected:?}, got {actual:?}"));
        }
    }
    assert!(problems.is_empty(), "{page}:\n{}", problems.join("\n"));
}

fn record(directory: &std::path::Path, page: &str, result: &PageResult) {
    fn escape(text: &str) -> String {
        let mut out = String::with_capacity(text.len() + 2);
        out.push('"');
        for c in text.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }
    let rows: Vec<String> = result
        .subtests
        .iter()
        .map(|(name, status)| {
            let (status, message) = match status {
                Status::NotRun => ("NOTRUN", None),
                Status::Pass => ("PASS", None),
                Status::Fail(message) => ("FAIL", Some(message.as_str())),
                Status::Timeout => ("TIMEOUT", None),
                Status::Unsupported(reason) => ("UNSUPPORTED", Some(reason.as_str())),
            };
            match message {
                Some(message) => format!(
                    "{{\"name\": {}, \"status\": \"{status}\", \"message\": {}}}",
                    escape(name),
                    escape(message)
                ),
                None => format!("{{\"name\": {}, \"status\": \"{status}\"}}", escape(name)),
            }
        })
        .collect();
    let optional = |text: &Option<String>| match text {
        Some(text) => escape(text),
        None => "null".into(),
    };
    let json = format!(
        "{{\"page\": {}, \"harness_error\": {}, \"unsupported\": {}, \"stopped\": {}, \"subtests\": [{}]}}\n",
        escape(page),
        optional(&result.harness_error),
        optional(&result.unsupported),
        optional(&result.stopped),
        rows.join(", ")
    );
    let file = directory.join(format!("{}.json", page.replace(['/', '.'], "_")));
    std::fs::create_dir_all(directory).expect("the record directory");
    std::fs::write(file, json).expect("the record file");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_as_javascript_prints_them() {
        for (value, text) in [
            (0.0, "0"),
            (-0.0, "0"),
            (1.0, "1"),
            (-1.5, "-1.5"),
            (0.1 + 0.2, "0.30000000000000004"),
            (1e21, "1e+21"),
            (123e19, "1.23e+21"),
            (1e20, "100000000000000000000"),
            (0.000001, "0.000001"),
            (1e-7, "1e-7"),
            (-1.5e-7, "-1.5e-7"),
            (1000.0 / 60.0, "16.666666666666668"),
            (f64::NAN, "NaN"),
            (f64::NEG_INFINITY, "-Infinity"),
        ] {
            assert_eq!(number_to_string(value), text, "{value:e}");
        }
    }

    #[test]
    fn strings_convert_as_string_to_number() {
        for (text, value) in [
            ("", 0.0),
            (" \t\n", 0.0),
            ("12", 12.0),
            (" -1.5e3 ", -1500.0),
            (".5", 0.5),
            ("5.", 5.0),
            ("+Infinity", f64::INFINITY),
            ("-Infinity", f64::NEG_INFINITY),
        ] {
            assert_eq!(string_to_number(text).unwrap(), value, "{text:?}");
        }
        for text in [
            "inf", "infinity", "NaN", "1e", "e5", ".", "1_000", "12px", "\u{85}1",
        ] {
            assert!(string_to_number(text).unwrap().is_nan(), "{text:?}");
        }
        assert!(string_to_number("\u{feff}7").unwrap() == 7.0);
        for text in ["0x10", "0b1", "0o7"] {
            assert!(matches!(string_to_number(text), Err(Throw::Unsupported(_))));
        }
        assert!(string_to_number("-0x10").unwrap().is_nan());
    }

    type Log = Rc<RefCell<Vec<&'static str>>>;

    fn note(log: &Log, text: &'static str) -> Value {
        let log = log.clone();
        Value::function("note", move |_, _, _| {
            log.borrow_mut().push(text);
            Ok(Value::Undefined)
        })
    }

    fn resolved(rt: &Rt) -> Value {
        Value::Host(Host::Promise(rt.promise_resolve(Value::Undefined)))
    }

    #[test]
    fn returning_a_promise_takes_the_thenable_job_ticks() {
        // Promise.resolve().then(() => { log(0); return Promise.resolve(); })
        //   .then(() => log(4));
        // Promise.resolve().then(() => log(1)).then(() => log(2)) ...
        let rt = Rt::new(None);
        let log = Log::default();
        let returns = {
            let log = log.clone();
            Value::function("", move |rt, _, _| {
                log.borrow_mut().push("0");
                Ok(resolved(rt))
            })
        };
        let first = rt.call_method(&resolved(&rt), "then", vec![returns]);
        rt.call_method(&first.unwrap(), "then", vec![note(&log, "4")])
            .unwrap();
        let mut chain = resolved(&rt);
        for text in ["1", "2", "3", "5", "6"] {
            chain = rt
                .call_method(&chain, "then", vec![note(&log, text)])
                .unwrap();
        }
        rt.checkpoint();
        assert_eq!(*log.borrow(), ["0", "1", "2", "3", "4", "5", "6"]);
    }

    #[test]
    fn an_async_body_runs_to_its_first_await() {
        let rt = Rt::new(None);
        let log = Log::default();
        let (inner, body_log) = (rt.clone(), log.clone());
        rt.spawn_async(async move {
            body_log.borrow_mut().push("a");
            inner.await_value(Value::Null).await?;
            body_log.borrow_mut().push("c");
            Ok(Value::Undefined)
        })
        .unwrap();
        log.borrow_mut().push("b");
        rt.call_method(&resolved(&rt), "then", vec![note(&log, "d")])
            .unwrap();
        rt.checkpoint();
        assert_eq!(*log.borrow(), ["a", "b", "c", "d"]);
    }

    #[test]
    fn an_unsupported_link_stops_the_chain_and_is_not_a_harness_error() {
        let rt = Rt::new(None);
        let log = Log::default();
        let stops = Value::function("", |rt, _, _| rt.unsupported("surface:Image"));
        let link = rt.call_method(&resolved(&rt), "then", vec![stops]).unwrap();
        rt.call_method(&link, "then", vec![note(&log, "later")])
            .unwrap();
        rt.checkpoint();
        assert!(log.borrow().is_empty());
        assert_eq!(rt.0.stopped.borrow().as_deref(), Some("surface:Image"));
        assert_eq!(*rt.0.harness_error.borrow(), None);
    }

    #[test]
    fn an_inline_style_outranks_an_adopted_sheet() {
        div::stylesheet!(sheet { #a { width: 30px; } });
        let rt = Rt::new(None);
        rt.adopt(&sheet::SHEET).unwrap();
        let element = rt.element("div", &[("id", "a")], None).unwrap();
        rt.frame();
        assert_eq!(element.client_width().unwrap(), 30);
        rt.set_style(&element, "width", &Value::str("40px"))
            .unwrap();
        rt.frame();
        assert_eq!(element.client_width().unwrap(), 40);
    }

    #[test]
    fn timeouts_convert_as_webidl_long() {
        assert_eq!(to_int32(1.9), 1);
        assert_eq!(to_int32(-1.9), -1);
        assert_eq!(to_int32(f64::NAN), 0);
        assert_eq!(to_int32(4_294_967_296.0 + 5.0), 5);
        assert_eq!(to_int32(2_147_483_648.0), -2_147_483_648);
    }
}
