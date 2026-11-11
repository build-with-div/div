//! Public C/raw Rust guards. extern C callbacks record failures; assertions run
//! only after callbacks return, so no test assertion unwinds through C.

use super::*;

struct Fixture {
    doc: *mut DivUiDocument,
    root: *mut DivUiElement,
    node: *mut DivUiElement,
    state: Rc<DocumentState>,
    native: div::Element,
}

fn text(value: &str) -> DivUiUtf8 {
    DivUiUtf8 {
        data: value.as_ptr(),
        length: value.len(),
    }
}

fn fixture(tag: i32) -> Fixture {
    let config = DivUiDocumentConfig {
        struct_size: size_of::<DivUiDocumentConfig>() as u32,
        abi_version: DIVUI_ABI_VERSION,
        viewport: DivUiViewportMetrics {
            logical_width: 100.0,
            logical_height: 80.0,
            physical_width: 100,
            physical_height: 80,
            device_scale_factor: 1.0,
            authority: 1,
            reserved: 0,
        },
    };
    let mut doc = ptr::null_mut();
    let mut root = ptr::null_mut();
    let mut node = ptr::null_mut();
    assert_eq!(divui_document_create(&config, &mut doc), DivUiStatus::Ok);
    assert_eq!(divui_document_root(doc, &mut root), DivUiStatus::Ok);
    assert_eq!(divui_element_create(doc, tag, &mut node), DivUiStatus::Ok);
    assert_eq!(divui_element_append_child(root, node), DivUiStatus::Ok);
    assert_eq!(
        divui_element_set_attribute(node, text("id"), text("raw-node")),
        DivUiStatus::Ok
    );
    let state = document(doc as usize).unwrap();
    let native = state.native.element_by_id("raw-node").unwrap().unwrap();
    Fixture {
        doc,
        root,
        node,
        state,
        native,
    }
}

impl Fixture {
    fn finish(self) {
        assert_eq!(divui_element_destroy(self.node), DivUiStatus::Ok);
        assert_eq!(divui_element_destroy(self.root), DivUiStatus::Ok);
        assert_eq!(divui_document_destroy(self.doc), DivUiStatus::Ok);
    }
}

#[derive(Debug)]
struct Observation {
    phase: u32,
    flags: u32,
    target: usize,
    current: usize,
    timestamp: u64,
    xy: (f32, f32),
    deltas: (f32, f32),
    modifiers: u32,
    key: i32,
    pointer: u32,
    text: String,
    properties_status: DivUiStatus,
    bubbles: u32,
    cancelable: u32,
}

#[derive(Default)]
struct Shared {
    order: RefCell<Vec<u8>>,
    observations: RefCell<Vec<Observation>>,
}

struct CallbackContext {
    shared: Rc<Shared>,
    tag: u8,
    request_flags: u32,
}

unsafe extern "C" fn record(event: *mut DivUiEvent, data: *mut c_void) {
    // SAFETY: event/context and the owned native text span this synchronous call.
    let (event, context) = unsafe { (&mut *event, &*data.cast::<CallbackContext>()) };
    let mut properties = DivUiEventPropertiesV1 {
        struct_size: size_of::<DivUiEventPropertiesV1>() as u32,
        abi_version: DIVUI_ABI_VERSION,
        bubbles: u32::MAX,
        cancelable: u32::MAX,
    };
    let status = divui_event_properties_v1(event, &mut properties);
    let value = if event.text.length == 0 {
        String::new()
    } else {
        // SAFETY: callback text is borrowed from the validated owned native value.
        String::from_utf8_lossy(unsafe {
            std::slice::from_raw_parts(event.text.data, event.text.length)
        })
        .into_owned()
    };
    context.shared.order.borrow_mut().push(context.tag);
    context.shared.observations.borrow_mut().push(Observation {
        phase: event.phase,
        flags: event.flags,
        target: event.target as usize,
        current: event.current_target as usize,
        timestamp: event.timestamp_ns,
        xy: (event.x, event.y),
        deltas: (event.delta_x, event.delta_y),
        modifiers: event.modifiers,
        key: event.key_code,
        pointer: event.pointer_id,
        text: value,
        properties_status: status,
        bubbles: properties.bubbles,
        cancelable: properties.cancelable,
    });
    event.flags |= context.request_flags;
}

fn add(
    node: *mut DivUiElement,
    kind: u32,
    capture: bool,
    callback: DivUiEventCallback,
    data: *mut c_void,
) -> *mut DivUiListener {
    let mut listener = ptr::null_mut();
    assert_eq!(
        divui_element_add_event_listener(
            node,
            i32::try_from(kind).unwrap(),
            u8::from(capture),
            Some(callback),
            data,
            &mut listener
        ),
        DivUiStatus::Ok
    );
    listener
}

fn event(kind: u32, value: &str) -> DivUiEvent {
    synthesized_event(kind, text(value))
}

#[test]
fn raw_c_wheel_uses_mixed_chronology_owned_payload_and_exact_c_target_alias() {
    let f = fixture(0);
    // Create a later alias: callbacks still receive the explicit caller's alias.
    let handle = element(f.node as usize).unwrap().node;
    let alias = focus_events::owned_element_handle(&f.state, handle).unwrap();
    let shared = Rc::new(Shared::default());
    let capture = CallbackContext {
        shared: shared.clone(),
        tag: 2,
        request_flags: 0,
    };
    let target = CallbackContext {
        shared: shared.clone(),
        tag: 4,
        request_flags: 0,
    };
    let bubble = CallbackContext {
        shared: shared.clone(),
        tag: 7,
        request_flags: 0,
    };
    let seen = shared.clone();
    f.state
        .native
        .body()
        .on_capture("wheel", move |_| seen.order.borrow_mut().push(1))
        .unwrap();
    let a = add(
        f.root,
        5,
        true,
        record,
        (&capture as *const CallbackContext).cast_mut().cast(),
    );
    let seen = shared.clone();
    f.native
        .on("wheel", move |event| {
            seen.order.borrow_mut().push(3);
            assert_eq!(event.timestamp_ns(), 812);
            assert_eq!(event.key_text, "native payload");
            assert_eq!(event.phase(), Some(div::EventPhase::Target));
        })
        .unwrap();
    let b = add(
        f.node,
        5,
        false,
        record,
        (&target as *const CallbackContext).cast_mut().cast(),
    );
    let seen = shared.clone();
    f.native
        .on("wheel", move |_| seen.order.borrow_mut().push(5))
        .unwrap();
    let seen = shared.clone();
    f.state
        .native
        .body()
        .on("wheel", move |_| seen.order.borrow_mut().push(6))
        .unwrap();
    let c = add(
        f.root,
        5,
        false,
        record,
        (&bubble as *const CallbackContext).cast_mut().cast(),
    );
    let mut raw = event(5, "native payload");
    raw.timestamp_ns = 812;
    raw.x = 1.25;
    raw.y = 3.5;
    raw.delta_x = -0.5;
    raw.delta_y = 8.75;
    raw.modifiers = 3;
    raw.key_code = 37;
    raw.pointer_id = 913;
    assert_eq!(
        divui_document_dispatch_event(f.doc, f.node, &mut raw),
        DivUiStatus::Ok
    );
    assert_eq!(*shared.order.borrow(), vec![1, 2, 3, 4, 5, 6, 7]);
    let records = shared.observations.borrow();
    for (observed, phase, current) in [
        (&records[0], 1, f.root),
        (&records[1], 2, f.node),
        (&records[2], 3, f.root),
    ] {
        assert_eq!(observed.phase, phase);
        assert_eq!(observed.current, current as usize);
        assert_eq!(observed.target, f.node as usize);
        assert_eq!(observed.timestamp, 812);
        assert_eq!(observed.xy, (1.25, 3.5));
        assert_eq!(observed.deltas, (-0.5, 8.75));
        assert_eq!(
            (observed.modifiers, observed.key, observed.pointer),
            (3, 37, 913)
        );
        assert_eq!(observed.text, "native payload");
        assert_eq!(observed.properties_status, DivUiStatus::Ok);
        assert_eq!((observed.bubbles, observed.cancelable), (1, 1));
    }
    drop(records);
    assert_eq!(raw.target, f.node);
    assert!(raw.current_target.is_null());
    assert_eq!(raw.phase, 3);
    for listener in [a, b, c] {
        assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
    }
    f.native.remove_event("wheel").unwrap();
    f.state.native.body().remove_event("wheel").unwrap();
    assert_eq!(divui_element_destroy(alias), DivUiStatus::Ok);
    f.finish();
}

#[test]
fn c_native_boundary_events_and_explicit_rust_options_share_actual_properties() {
    for (kind, name, bubbles) in [
        (14, "pointerenter", 0),
        (15, "pointerleave", 0),
        (24, "pointercancel", 1),
    ] {
        let f = fixture(0);
        let shared = Rc::new(Shared::default());
        let target = CallbackContext {
            shared: shared.clone(),
            tag: 2,
            request_flags: DIVUI_EVENT_FLAG_DEFAULT_PREVENTED,
        };
        let bubble = CallbackContext {
            shared: shared.clone(),
            tag: 4,
            request_flags: 0,
        };
        let seen = shared.clone();
        f.state
            .native
            .body()
            .on_capture(name, move |event| {
                seen.order.borrow_mut().push(1);
                event.prevent_default();
            })
            .unwrap();
        let a = add(
            f.node,
            kind,
            false,
            record,
            (&target as *const CallbackContext).cast_mut().cast(),
        );
        let seen = shared.clone();
        f.native
            .on(name, move |event| {
                seen.order.borrow_mut().push(3);
                event.prevent_default();
            })
            .unwrap();
        let b = add(
            f.root,
            kind,
            false,
            record,
            (&bubble as *const CallbackContext).cast_mut().cast(),
        );
        let mut raw = event(kind, "");
        assert_eq!(
            divui_document_dispatch_event(f.doc, f.node, &mut raw),
            DivUiStatus::Ok
        );
        assert_eq!(
            *shared.order.borrow(),
            if bubbles == 0 {
                vec![1, 2, 3]
            } else {
                vec![1, 2, 3, 4]
            }
        );
        assert_eq!(raw.flags & DIVUI_EVENT_FLAG_DEFAULT_PREVENTED, 0);
        for observed in shared.observations.borrow().iter() {
            assert_eq!(observed.properties_status, DivUiStatus::Ok);
            assert_eq!((observed.bubbles, observed.cancelable), (bubbles, 0));
            assert_eq!(observed.flags & DIVUI_EVENT_FLAG_DEFAULT_PREVENTED, 0);
        }
        shared.order.borrow_mut().clear();
        shared.observations.borrow_mut().clear();
        let explicit = div::Event::new(name, true, true);
        assert!(!f.state.native.dispatch_event(&f.native, &explicit).unwrap());
        assert!(explicit.default_prevented());
        assert_eq!(*shared.order.borrow(), vec![1, 2, 3, 4]);
        for observed in shared.observations.borrow().iter() {
            assert_eq!((observed.bubbles, observed.cancelable), (1, 1));
        }
        assert_eq!(divui_listener_destroy(a), DivUiStatus::Ok);
        assert_eq!(divui_listener_destroy(b), DivUiStatus::Ok);
        f.native.remove_event(name).unwrap();
        f.state.native.body().remove_event(name).unwrap();
        f.finish();
    }
}

struct NestedContext {
    doc: *mut DivUiDocument,
    node: *mut DivUiElement,
    victim: Cell<*mut DivUiListener>,
    replacement: Cell<*mut DivUiListener>,
    changed: Cell<bool>,
    nested: bool,
    order: RefCell<Vec<u8>>,
    statuses: RefCell<Vec<DivUiStatus>>,
    outer: Cell<*const DivUiEvent>,
}

unsafe extern "C" fn old_listener(_: *mut DivUiEvent, data: *mut c_void) {
    // SAFETY: context lives until all registrations and dispatches end.
    unsafe { &*data.cast::<NestedContext>() }
        .order
        .borrow_mut()
        .push(2);
}

unsafe extern "C" fn new_listener(_: *mut DivUiEvent, data: *mut c_void) {
    // SAFETY: context lives until all registrations and dispatches end.
    let context = unsafe { &*data.cast::<NestedContext>() };
    context.order.borrow_mut().push(3);
    let mut properties = DivUiEventPropertiesV1 {
        struct_size: size_of::<DivUiEventPropertiesV1>() as u32,
        abi_version: DIVUI_ABI_VERSION,
        bubbles: 0,
        cancelable: 0,
    };
    // Query the still active outer event during a nested event callback.
    if !context.outer.get().is_null() {
        context
            .statuses
            .borrow_mut()
            .push(divui_event_properties_v1(
                context.outer.get(),
                &mut properties,
            ));
    }
}

unsafe extern "C" fn remove_readd_and_nest(event: *mut DivUiEvent, data: *mut c_void) {
    // SAFETY: distinct nested descriptors and context live through each callback.
    let context = unsafe { &*data.cast::<NestedContext>() };
    context.order.borrow_mut().push(1);
    if context.changed.replace(true) {
        return;
    }
    context
        .statuses
        .borrow_mut()
        .push(divui_listener_destroy(context.victim.get()));
    let mut replacement = ptr::null_mut();
    let status = divui_element_add_event_listener(
        context.node,
        5,
        0,
        Some(new_listener),
        data,
        &mut replacement,
    );
    context.statuses.borrow_mut().push(status);
    context.replacement.set(replacement);
    if context.nested {
        context.outer.set(event);
        let mut nested = synthesized_event(
            5,
            DivUiUtf8 {
                data: ptr::null(),
                length: 0,
            },
        );
        let status = divui_document_dispatch_event(context.doc, context.node, &mut nested);
        context.statuses.borrow_mut().push(status);
        context.outer.set(ptr::null());
    }
}

#[test]
fn raw_c_remove_readd_nested_dispatch_uses_live_ids_and_preserves_outer_metadata() {
    for nested in [false, true] {
        let f = fixture(0);
        let context = NestedContext {
            doc: f.doc,
            node: f.node,
            victim: Cell::new(ptr::null_mut()),
            replacement: Cell::new(ptr::null_mut()),
            changed: Cell::new(false),
            nested,
            order: RefCell::new(Vec::new()),
            statuses: RefCell::new(Vec::new()),
            outer: Cell::new(ptr::null()),
        };
        let data = (&context as *const NestedContext).cast_mut().cast();
        let first = add(f.node, 5, false, remove_readd_and_nest, data);
        context
            .victim
            .set(add(f.node, 5, false, old_listener, data));
        let mut raw = event(5, "");
        assert_eq!(
            divui_document_dispatch_event(f.doc, f.node, &mut raw),
            DivUiStatus::Ok
        );
        assert_eq!(
            *context.order.borrow(),
            if nested { vec![1, 1, 3] } else { vec![1] }
        );
        assert!(context
            .statuses
            .borrow()
            .iter()
            .all(|status| *status == DivUiStatus::Ok));
        context.order.borrow_mut().clear();
        assert_eq!(
            divui_document_dispatch_event(f.doc, f.node, &mut raw),
            DivUiStatus::Ok
        );
        assert_eq!(*context.order.borrow(), vec![1, 3]);
        assert_eq!(divui_listener_destroy(first), DivUiStatus::Ok);
        assert_eq!(
            divui_listener_destroy(context.replacement.get()),
            DivUiStatus::Ok
        );
        assert_eq!(
            divui_listener_destroy(context.victim.get()),
            DivUiStatus::InvalidHandle
        );
        f.finish();
    }
}

#[test]
fn raw_c_pointer_kind_preserves_rust_mouse_and_pointer_listener_distinction() {
    let f = fixture(0);
    let shared = Rc::new(Shared::default());
    let c = CallbackContext {
        shared: shared.clone(),
        tag: 2,
        request_flags: 0,
    };
    let seen = shared.clone();
    f.native
        .on("mousedown", move |_| seen.order.borrow_mut().push(1))
        .unwrap();
    let listener = add(
        f.node,
        1,
        false,
        record,
        (&c as *const CallbackContext).cast_mut().cast(),
    );
    let seen = shared.clone();
    f.native
        .on("pointerdown", move |_| seen.order.borrow_mut().push(3))
        .unwrap();
    let mut raw = event(1, "");
    assert_eq!(
        divui_document_dispatch_event(f.doc, f.node, &mut raw),
        DivUiStatus::Ok
    );
    assert_eq!(*shared.order.borrow(), vec![2, 3]);
    shared.order.borrow_mut().clear();
    f.state
        .native
        .dispatch_event(&f.native, &div::Event::native("mousedown", 0))
        .unwrap();
    assert_eq!(*shared.order.borrow(), vec![1, 2]);
    assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
    f.native.remove_event("mousedown").unwrap();
    f.native.remove_event("pointerdown").unwrap();
    f.finish();
}

#[test]
fn ordinary_raw_c_stop_flags_preserve_same_target_listeners_and_skip_ancestors() {
    for immediate in [false, true] {
        let f = fixture(0);
        let shared = Rc::new(Shared::default());
        let request = if immediate {
            DIVUI_EVENT_FLAG_IMMEDIATE_PROPAGATION_STOPPED
        } else {
            DIVUI_EVENT_FLAG_PROPAGATION_STOPPED
        };
        let c = CallbackContext {
            shared: shared.clone(),
            tag: 2,
            request_flags: request,
        };
        let seen = shared.clone();
        f.native
            .on("wheel", move |_| seen.order.borrow_mut().push(1))
            .unwrap();
        let listener = add(
            f.node,
            5,
            false,
            record,
            (&c as *const CallbackContext).cast_mut().cast(),
        );
        let seen = shared.clone();
        f.native
            .on("wheel", move |_| seen.order.borrow_mut().push(3))
            .unwrap();
        let seen = shared.clone();
        f.state
            .native
            .body()
            .on("wheel", move |_| seen.order.borrow_mut().push(4))
            .unwrap();
        let mut raw = event(5, "");
        assert_eq!(
            divui_document_dispatch_event(f.doc, f.node, &mut raw),
            DivUiStatus::Ok
        );
        assert_eq!(
            *shared.order.borrow(),
            if immediate { vec![1, 2] } else { vec![1, 2, 3] }
        );
        assert_ne!(raw.flags & DIVUI_EVENT_FLAG_PROPAGATION_STOPPED, 0);
        assert_eq!(
            raw.flags & DIVUI_EVENT_FLAG_IMMEDIATE_PROPAGATION_STOPPED != 0,
            immediate
        );
        assert_eq!(raw.phase, 2);
        assert!(raw.current_target.is_null());
        assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
        f.native.remove_event("wheel").unwrap();
        f.state.native.body().remove_event("wheel").unwrap();
        f.finish();
    }
}

#[test]
fn raw_scroll_button_event_keeps_owned_raw_target_and_projects_public_origin() {
    let f = fixture(0);
    let mut button = ptr::null_mut();
    assert_eq!(
        divui_element_create_scroll_button_v1(f.node, 0, &mut button),
        DivUiStatus::Ok
    );
    let raw_node = element(button as usize).unwrap().node;
    let origin_node = element(f.node as usize).unwrap().node;
    assert_ne!(raw_node, origin_node);
    let shared = Rc::new(Shared::default());
    let c = CallbackContext {
        shared: shared.clone(),
        tag: 2,
        request_flags: 0,
    };
    let seen = shared.clone();
    f.native
        .on("wheel", move |event| {
            seen.order.borrow_mut().push(1);
            assert_eq!(
                event
                    .target()
                    .unwrap()
                    .get_attribute("id")
                    .unwrap()
                    .as_deref(),
                Some("raw-node")
            );
        })
        .unwrap();
    let listener = add(
        f.node,
        5,
        false,
        record,
        (&c as *const CallbackContext).cast_mut().cast(),
    );
    let mut raw = event(5, "");
    assert_eq!(
        divui_document_dispatch_event(f.doc, button, &mut raw),
        DivUiStatus::Ok
    );
    assert_eq!(*shared.order.borrow(), vec![1, 2]);
    let records = shared.observations.borrow();
    assert_eq!(records[0].target, f.node as usize);
    assert_eq!(records[0].current, f.node as usize);
    drop(records);
    assert_eq!(raw.target, f.node);
    // Projection never substitutes the caller's retained native button handle.
    assert_eq!(element(button as usize).unwrap().node, raw_node);
    assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
    f.native.remove_event("wheel").unwrap();
    assert_eq!(divui_element_destroy(button), DivUiStatus::Ok);
    f.finish();
}

#[test]
fn rust_cancellation_reaches_legacy_c_text_and_composition_defaults() {
    let f = fixture(23);
    assert_eq!(
        divui_element_set_control_value(f.node, text("kept")),
        DivUiStatus::Ok
    );
    let seen = Rc::new(RefCell::new(Vec::new()));
    let cancel = Rc::new(Cell::new(true));
    for name in [
        "textinput",
        "compositionstart",
        "compositionupdate",
        "compositionend",
    ] {
        let log = seen.clone();
        let cancel = cancel.clone();
        f.native
            .on(name, move |event| {
                log.borrow_mut().push(event.event_type.clone());
                assert!(event.input_info().is_none());
                if cancel.get() {
                    event.prevent_default();
                }
            })
            .unwrap();
    }
    let mut raw = event(8, "ignored");
    assert_eq!(
        divui_document_dispatch_event(f.doc, f.node, &mut raw),
        DivUiStatus::Ok
    );
    assert_ne!(raw.flags & DIVUI_EVENT_FLAG_DEFAULT_PREVENTED, 0);
    assert_eq!(f.native.control_value().unwrap().as_deref(), Some("kept"));
    cancel.set(false);
    assert_eq!(divui_element_set_selection(f.node, 0, 4), DivUiStatus::Ok);
    for (kind, value) in [(9, ""), (10, "preview"), (11, "final")] {
        let mut raw = event(kind, value);
        assert_eq!(
            divui_document_dispatch_event(f.doc, f.node, &mut raw),
            DivUiStatus::Ok
        );
    }
    assert_eq!(f.native.control_value().unwrap().as_deref(), Some("final"));
    assert_eq!(
        *seen.borrow(),
        vec![
            "textinput",
            "compositionstart",
            "compositionupdate",
            "compositionend"
        ]
    );
    for name in [
        "textinput",
        "compositionstart",
        "compositionupdate",
        "compositionend",
    ] {
        f.native.remove_event(name).unwrap();
    }
    f.finish();
}

#[test]
fn c_raw_event_prefix_checks_precede_full_layout_and_preserve_error_storage() {
    let f = fixture(0);
    for pointer_source in [false, true] {
        let mut short = size_of::<u32>() as u32;
        let address = (&mut short as *mut u32).cast::<DivUiEvent>();
        let status = if pointer_source {
            divui_document_dispatch_pointer_event(f.doc, address)
        } else {
            divui_document_dispatch_event(f.doc, f.node, address)
        };
        assert_eq!(status, DivUiStatus::InvalidArgument);
        assert_eq!(short, 4);
        let mut header = [8u32, DIVUI_ABI_VERSION];
        let address = header.as_mut_ptr().cast::<DivUiEvent>();
        let status = if pointer_source {
            divui_document_dispatch_pointer_event(f.doc, address)
        } else {
            divui_document_dispatch_event(f.doc, f.node, address)
        };
        assert_eq!(status, DivUiStatus::InvalidArgument);
        assert_eq!(header, [8, DIVUI_ABI_VERSION]);
    }
    #[repr(C)]
    struct Extended {
        event: DivUiEvent,
        sentinel: [u8; 24],
    }
    let mut extended = Extended {
        event: event(5, ""),
        sentinel: [0xa7; 24],
    };
    extended.event.struct_size = size_of::<Extended>() as u32;
    assert_eq!(
        divui_document_dispatch_event(f.doc, f.node, &mut extended.event),
        DivUiStatus::Ok
    );
    assert_eq!(extended.sentinel, [0xa7; 24]);
    f.finish();
}
