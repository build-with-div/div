use super::*;

fn focus_info() -> DivUiFocusEventInfoV1 {
    DivUiFocusEventInfoV1 {
        struct_size: size_of::<DivUiFocusEventInfoV1>() as u32,
        abi_version: DIVUI_ABI_VERSION,
        bubbles: 0,
        cancelable: 0,
        related_target: ptr::null_mut(),
    }
}

struct MetadataCallback {
    document: *mut DivUiDocument,
    nodes: [*mut DivUiElement; 3],
    outer: Cell<usize>,
    retained: Cell<*mut DivUiElement>,
    rows: RefCell<Vec<(&'static str, Option<NodeHandle>, Option<NodeHandle>)>>,
    failures: RefCell<Vec<String>>,
}

impl MetadataCallback {
    // These helpers record failures rather than unwinding through a C callback.
    fn status(&self, label: &str, actual: DivUiStatus, expected: DivUiStatus) -> bool {
        if actual == expected {
            true
        } else {
            self.failures
                .borrow_mut()
                .push(format!("{label}: {actual:?}, expected {expected:?}"));
            false
        }
    }

    fn observe(&self, label: &'static str, event: *const DivUiEvent) {
        #[repr(C)]
        struct ExtendedInfo {
            info: DivUiFocusEventInfoV1,
            trailing: [u8; 16],
        }
        let mut extended = ExtendedInfo {
            info: focus_info(),
            trailing: [0x6d; 16],
        };
        extended.info.struct_size = size_of::<ExtendedInfo>() as u32;
        if !self.status(
            "active metadata",
            divui_event_focus_info_v1(event, &mut extended.info),
            DivUiStatus::Ok,
        ) {
            return;
        }
        if extended.trailing != [0x6d; 16]
            || extended.info.bubbles != 0
            || extended.info.cancelable != 0
        {
            self.failures.borrow_mut().push("metadata fields".into());
        }
        let related = if extended.info.related_target.is_null() {
            None
        } else {
            element(extended.info.related_target as usize)
                .ok()
                .map(|value| value.node)
        };
        let mut focused = ptr::null_mut();
        self.status(
            "focused snapshot",
            divui_document_focused_element_v1(self.document, &mut focused),
            DivUiStatus::Ok,
        );
        let active = if focused.is_null() {
            None
        } else {
            element(focused as usize).ok().map(|value| value.node)
        };
        self.rows.borrow_mut().push((label, related, active));
        if !focused.is_null() {
            self.status(
                "destroy focused snapshot",
                divui_element_destroy(focused),
                DivUiStatus::Ok,
            );
        }
        if !extended.info.related_target.is_null() {
            if self.retained.get().is_null() {
                self.retained.set(extended.info.related_target);
            } else {
                self.status(
                    "destroy related snapshot",
                    divui_element_destroy(extended.info.related_target),
                    DivUiStatus::Ok,
                );
            }
        }
    }
}

unsafe extern "C" fn nested_metadata(event: *mut DivUiEvent, user_data: *mut c_void) {
    // SAFETY: synchronous listener dispatch owns the event and context until
    // return. No context RefCell borrow is held across the nested focus call.
    let data = unsafe { &*user_data.cast::<MetadataCallback>() };
    let value = unsafe { &*event };
    if value.target == data.nodes[0] && value.event_type == 13 {
        data.outer.set(event as usize);
        data.observe("outer-before", event);
        let address = event as usize;
        let document_address = data.document as usize;
        let wrong_thread = std::thread::spawn(move || {
            let mut info = focus_info();
            let mut focused = ptr::null_mut();
            (
                divui_event_focus_info_v1(address as *const DivUiEvent, &mut info),
                divui_document_focused_element_v1(
                    document_address as *mut DivUiDocument,
                    &mut focused,
                ),
            )
        })
        .join();
        match wrong_thread {
            Ok((metadata, focused)) => {
                data.status(
                    "metadata outside owning thread",
                    metadata,
                    DivUiStatus::InvalidState,
                );
                data.status(
                    "focus outside owning thread",
                    focused,
                    DivUiStatus::WrongThread,
                );
            }
            Err(_) => data.failures.borrow_mut().push("thread panic".into()),
        }
        data.status(
            "nested focus",
            divui_element_focus(data.nodes[2]),
            DivUiStatus::Ok,
        );
        data.observe("outer-after", event);
    } else if value.target == data.nodes[2] && value.event_type == 12 {
        data.observe("nested", event);
        data.observe("outer-during-nested", data.outer.get() as *const DivUiEvent);
    }
}

#[test]
fn c_focus_metadata_preserves_nested_scopes_threads_owned_handles_and_future_bytes() {
    let fixture = Fixture::new();
    let context = MetadataCallback {
        document: fixture.document,
        nodes: fixture.nodes,
        outer: Cell::new(0),
        retained: Cell::new(ptr::null_mut()),
        rows: RefCell::new(Vec::new()),
        failures: RefCell::new(Vec::new()),
    };
    let mut listeners = Vec::new();
    for (node, event_type) in [(fixture.nodes[0], 13), (fixture.nodes[2], 12)] {
        let mut listener = ptr::null_mut();
        assert_eq!(
            divui_element_add_event_listener(
                node,
                event_type,
                0,
                Some(nested_metadata),
                (&context as *const MetadataCallback).cast_mut().cast(),
                &mut listener,
            ),
            DivUiStatus::Ok
        );
        listeners.push(listener);
    }
    assert_eq!(divui_element_focus(fixture.nodes[0]), DivUiStatus::Ok);
    assert_eq!(divui_element_focus(fixture.nodes[1]), DivUiStatus::Ok);
    let b = element(fixture.nodes[1] as usize).unwrap().node;
    let c = element(fixture.nodes[2] as usize).unwrap().node;
    assert_eq!(
        *context.rows.borrow(),
        [
            ("outer-before", Some(b), None),
            ("nested", None, Some(c)),
            ("outer-during-nested", Some(b), Some(c)),
            ("outer-after", Some(b), Some(c)),
        ]
    );
    assert!(
        context.failures.borrow().is_empty(),
        "{:?}",
        context.failures.borrow()
    );
    let mut expired = focus_info();
    assert_eq!(
        divui_event_focus_info_v1(context.outer.get() as *const DivUiEvent, &mut expired),
        DivUiStatus::InvalidState
    );
    let retained = context.retained.get();
    assert!(!retained.is_null());
    assert_eq!(divui_element_detach(fixture.nodes[1]), DivUiStatus::Ok);
    assert_eq!(element(retained as usize).unwrap().node, b);
    assert_eq!(divui_element_remove(fixture.nodes[1]), DivUiStatus::Ok);
    let mut replacement = ptr::null_mut();
    assert_eq!(
        divui_element_create(fixture.document, 23, &mut replacement),
        DivUiStatus::Ok
    );
    assert_eq!(
        divui_element_append_child(fixture.root, replacement),
        DivUiStatus::Ok
    );
    assert_ne!(element(replacement as usize).unwrap().node, b);
    let mut length = 0;
    assert_eq!(
        divui_element_copy_control_value(retained, ptr::null_mut(), 0, &mut length),
        DivUiStatus::StaleHandle
    );
    assert_eq!(divui_element_destroy(replacement), DivUiStatus::Ok);
    for listener in listeners {
        assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
    }
    drop(fixture);
    assert_eq!(
        divui_element_copy_control_value(retained, ptr::null_mut(), 0, &mut length),
        DivUiStatus::InvalidHandle
    );
    assert_eq!(divui_element_destroy(retained), DivUiStatus::Ok);
}

#[test]
fn c_focus_metadata_rejects_bad_headers_and_leaves_outputs_unchanged_on_failure() {
    assert_eq!(
        divui_event_focus_info_v1(ptr::null(), ptr::null_mut()),
        DivUiStatus::InvalidArgument
    );
    let mut info = focus_info();
    info.struct_size = 4;
    assert_eq!(
        divui_event_focus_info_v1(ptr::null(), &mut info),
        DivUiStatus::InvalidArgument
    );
    info.struct_size = size_of::<DivUiFocusEventInfoV1>() as u32;
    info.abi_version ^= 1;
    assert_eq!(
        divui_event_focus_info_v1(ptr::null(), &mut info),
        DivUiStatus::AbiMismatch
    );
    info.abi_version = DIVUI_ABI_VERSION;
    info.bubbles = 0x1234;
    info.cancelable = 0x5678;
    info.related_target = 0x99usize as *mut DivUiElement;
    assert_eq!(
        divui_event_focus_info_v1(ptr::null(), &mut info),
        DivUiStatus::InvalidState
    );
    assert_eq!(info.bubbles, 0x1234);
    assert_eq!(info.cancelable, 0x5678);
    assert_eq!(info.related_target as usize, 0x99);
    let fixture = Fixture::new();
    let mut focused = 0x99usize as *mut DivUiElement;
    let state = document(fixture.document as usize).unwrap();
    let borrow = state.engine.borrow_mut();
    assert_eq!(
        divui_document_focused_element_v1(fixture.document, &mut focused),
        DivUiStatus::Reentrant
    );
    assert_eq!(focused as usize, 0x99);
    drop(borrow);
    assert_eq!(
        divui_document_focused_element_v1(fixture.document, &mut focused),
        DivUiStatus::Ok
    );
    assert!(focused.is_null());
}

struct SyntheticMetadata {
    calls: Cell<usize>,
    failed: Cell<bool>,
}

unsafe extern "C" fn synthetic_metadata(event: *mut DivUiEvent, user_data: *mut c_void) {
    // SAFETY: caller owns this context throughout synchronous dispatch.
    let data = unsafe { &*user_data.cast::<SyntheticMetadata>() };
    let mut info = focus_info();
    if divui_event_focus_info_v1(event, &mut info) != DivUiStatus::Ok
        || info.bubbles != 1
        || info.cancelable != 0
        || !info.related_target.is_null()
    {
        data.failed.set(true);
    }
    if !info.related_target.is_null() {
        if divui_element_destroy(info.related_target) != DivUiStatus::Ok {
            data.failed.set(true);
        }
    }
    data.calls.set(data.calls.get() + 1);
    // SAFETY: mutable borrowed event remains alive through this callback.
    unsafe { (*event).flags |= DIVUI_EVENT_FLAG_DEFAULT_PREVENTED };
}

#[test]
fn c_explicit_focus_notifications_share_native_rust_listeners_without_changing_focus() {
    let fixture = Fixture::new();
    assert_eq!(divui_element_focus(fixture.nodes[0]), DivUiStatus::Ok);
    let context = SyntheticMetadata {
        calls: Cell::new(0),
        failed: Cell::new(false),
    };
    let observations = Rc::new(RefCell::new(Vec::new()));
    let native = fixture.native("b");
    let mut listeners = Vec::new();
    for (name, event_type) in [("focusin", 25), ("focusout", 26)] {
        let observed = observations.clone();
        native
            .on(name, move |event| {
                observed.borrow_mut().push((
                    event.event_type.clone(),
                    event.target().unwrap().get_attribute("id").unwrap(),
                    event.bubbles(),
                    event.cancelable(),
                    event.default_prevented(),
                    event.related_target().is_none(),
                ));
            })
            .unwrap();
        let mut listener = ptr::null_mut();
        assert_eq!(
            divui_element_add_event_listener(
                fixture.nodes[1],
                event_type,
                0,
                Some(synthetic_metadata),
                (&context as *const SyntheticMetadata).cast_mut().cast(),
                &mut listener,
            ),
            DivUiStatus::Ok
        );
        listeners.push(listener);
        let mut event = synthesized_event(event_type as u32, text(""));
        assert_eq!(
            divui_document_dispatch_event(fixture.document, fixture.nodes[1], &mut event),
            DivUiStatus::Ok
        );
        assert_eq!(event.flags, 0);
        assert!(event.current_target.is_null());
        assert!(fixture.native("a").has_focus().unwrap());
        assert!(!native.has_focus().unwrap());
    }
    assert_eq!(context.calls.get(), 2);
    assert!(!context.failed.get());
    assert_eq!(
        *observations.borrow(),
        [
            ("focusin".into(), Some("b".into()), true, false, false, true),
            (
                "focusout".into(),
                Some("b".into()),
                true,
                false,
                false,
                true
            ),
        ]
    );
    for listener in listeners {
        assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
    }
}

fn text(value: &str) -> DivUiUtf8 {
    DivUiUtf8 {
        data: value.as_ptr(),
        length: value.len(),
    }
}

struct Fixture {
    document: *mut DivUiDocument,
    root: *mut DivUiElement,
    nodes: [*mut DivUiElement; 3],
}

impl Fixture {
    fn new() -> Self {
        let config = DivUiDocumentConfig {
            struct_size: size_of::<DivUiDocumentConfig>() as u32,
            abi_version: DIVUI_ABI_VERSION,
            viewport: DivUiViewportMetrics {
                logical_width: 320.0,
                logical_height: 200.0,
                physical_width: 320,
                physical_height: 200,
                device_scale_factor: 1.0,
                authority: 1,
                reserved: 0,
            },
        };
        let mut fixture = Self {
            document: ptr::null_mut(),
            root: ptr::null_mut(),
            nodes: [ptr::null_mut(); 3],
        };
        assert_eq!(
            divui_document_create(&config, &mut fixture.document),
            DivUiStatus::Ok
        );
        assert_eq!(
            divui_document_root(fixture.document, &mut fixture.root),
            DivUiStatus::Ok
        );
        for (node, id) in fixture.nodes.iter_mut().zip(["a", "b", "c"]) {
            assert_eq!(
                divui_element_create(fixture.document, 23, node),
                DivUiStatus::Ok
            );
            assert_eq!(
                divui_element_append_child(fixture.root, *node),
                DivUiStatus::Ok
            );
            assert_eq!(
                divui_element_set_attribute(*node, text("id"), text(id)),
                DivUiStatus::Ok
            );
        }
        fixture
    }

    fn native(&self, id: &str) -> div::Element {
        document(self.document as usize)
            .unwrap()
            .native
            .element_by_id(id)
            .unwrap()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for node in self.nodes {
            assert_eq!(divui_element_destroy(node), DivUiStatus::Ok);
        }
        assert_eq!(divui_element_destroy(self.root), DivUiStatus::Ok);
        assert_eq!(divui_document_destroy(self.document), DivUiStatus::Ok);
    }
}

struct Callback {
    document: *mut DivUiDocument,
    nodes: [*mut DivUiElement; 3],
    events: RefCell<Vec<(usize, u32, Option<String>)>>,
    redirect: Cell<bool>,
    failed: Cell<bool>,
}

unsafe extern "C" fn observe(event: *mut DivUiEvent, user_data: *mut c_void) {
    // SAFETY: the fixture owns the event and immutable context throughout
    // synchronous dispatch. Interior borrows end before any reentrant call.
    let event = unsafe { &*event };
    let data = unsafe { &*user_data.cast::<Callback>() };
    let Some(index) = data.nodes.iter().position(|node| *node == event.target) else {
        data.failed.set(true);
        return;
    };
    let focused = document(data.document as usize)
        .unwrap()
        .native
        .focused_element()
        .unwrap()
        .map(|node| node.get_attribute("id").unwrap().unwrap());
    data.events
        .borrow_mut()
        .push((index, event.event_type, focused));
    if event.target != event.current_target
        || divui_element_set_attribute(data.nodes[2], text("data-event"), text("seen"))
            != DivUiStatus::Ok
    {
        data.failed.set(true);
    }
    if index == 0
        && event.event_type == 13
        && data.redirect.replace(false)
        && divui_element_focus(data.nodes[2]) != DivUiStatus::Ok
    {
        data.failed.set(true);
    }
}

fn listen(fixture: &Fixture, context: &Callback) -> Vec<*mut DivUiListener> {
    let mut listeners = Vec::new();
    for node in fixture.nodes {
        for event in [12, 13, 11] {
            let mut listener = ptr::null_mut();
            assert_eq!(
                divui_element_add_event_listener(
                    node,
                    event,
                    0,
                    Some(observe),
                    (context as *const Callback).cast_mut().cast(),
                    &mut listener
                ),
                DivUiStatus::Ok
            );
            listeners.push(listener);
        }
    }
    listeners
}

#[test]
fn c_focus_and_blur_share_rust_callbacks_state_and_composition_cancellation() {
    let fixture = Fixture::new();
    let state = document(fixture.document as usize).unwrap();
    let context = Callback {
        document: fixture.document,
        nodes: fixture.nodes,
        events: RefCell::new(Vec::new()),
        redirect: Cell::new(false),
        failed: Cell::new(false),
    };
    let listeners = listen(&fixture, &context);
    let [a, b, c] = fixture.nodes;
    assert_eq!(divui_element_focus(a), DivUiStatus::Ok);
    assert_eq!(divui_element_focus(a), DivUiStatus::Ok);
    assert_eq!(divui_element_blur(b), DivUiStatus::Ok);
    fixture.native("a").set_control_value("kept").unwrap();
    state.native.dispatch_composition_start().unwrap();
    state.native.dispatch_composition_update("preview").unwrap();
    assert_eq!(divui_element_focus(b), DivUiStatus::Ok);
    assert_eq!(
        fixture.native("a").control_value().unwrap().as_deref(),
        Some("kept")
    );
    fixture.native("c").focus().unwrap();
    assert_eq!(
        divui_element_perform_accessibility_action(c, 2, text(""), 0, 0),
        DivUiStatus::Ok
    );
    assert_eq!(
        divui_element_perform_accessibility_action(a, 1, text(""), 0, 0),
        DivUiStatus::Ok
    );
    assert_eq!(
        *context.events.borrow(),
        [
            (0, 12, Some("a".into())),
            (0, 11, Some("a".into())),
            (0, 13, None),
            (1, 12, Some("b".into())),
            (1, 13, None),
            (2, 12, Some("c".into())),
            (2, 13, None),
            (0, 12, Some("a".into())),
        ]
    );
    assert!(!context.failed.get());
    context.events.borrow_mut().clear();
    context.redirect.set(true);
    assert_eq!(divui_element_focus(b), DivUiStatus::Ok);
    assert_eq!(
        *context.events.borrow(),
        [(0, 13, None), (2, 12, Some("c".into()))]
    );
    assert!(fixture.native("c").has_focus().unwrap());
    for listener in listeners {
        assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
    }
}

#[test]
fn c_focus_and_blur_validate_ownership_borrows_and_contain_callback_panics() {
    let fixture = Fixture::new();
    let [a, b, _] = fixture.nodes;
    assert_eq!(
        divui_element_focus(ptr::null_mut()),
        DivUiStatus::InvalidArgument
    );
    assert_eq!(
        divui_element_blur(ptr::null_mut()),
        DivUiStatus::InvalidArgument
    );
    let state = document(fixture.document as usize).unwrap();
    let borrow = state.engine.borrow_mut();
    assert_eq!(divui_element_focus(a), DivUiStatus::Reentrant);
    assert_eq!(divui_element_blur(a), DivUiStatus::Reentrant);
    drop(borrow);
    let address = a as usize;
    assert_eq!(
        std::thread::spawn(move || (
            divui_element_focus(address as *mut DivUiElement),
            divui_element_blur(address as *mut DivUiElement),
        ))
        .join()
        .unwrap(),
        (DivUiStatus::WrongThread, DivUiStatus::WrongThread)
    );
    assert_eq!(
        divui_element_set_attribute(b, text("disabled"), text("")),
        DivUiStatus::Ok
    );
    assert_eq!(divui_element_focus(b), DivUiStatus::InvalidState);
    let native = fixture.native("a");
    native
        .on("focus", |_| panic!("focus callback containment"))
        .unwrap();
    assert_eq!(divui_element_focus(a), DivUiStatus::Internal);
    assert!(native.has_focus().unwrap());
    native.remove_event("focus").unwrap();
    native
        .on("blur", |_| panic!("blur callback containment"))
        .unwrap();
    assert_eq!(divui_element_blur(a), DivUiStatus::Internal);
    assert!(state.native.focused_element().unwrap().is_none());
    native.remove_event("blur").unwrap();
    assert_eq!(divui_element_focus(a), DivUiStatus::Ok);
    assert_eq!(divui_element_blur(a), DivUiStatus::Ok);
    assert_eq!(divui_element_remove(a), DivUiStatus::Ok);
    assert_eq!(divui_element_focus(a), DivUiStatus::StaleHandle);
    assert_eq!(divui_element_blur(a), DivUiStatus::StaleHandle);
}

#[test]
fn c_sequential_and_modal_focus_share_callbacks_composition_and_final_output() {
    let fixture = Fixture::new();
    let state = document(fixture.document as usize).unwrap();
    let context = Callback {
        document: fixture.document,
        nodes: fixture.nodes,
        events: RefCell::new(Vec::new()),
        redirect: Cell::new(false),
        failed: Cell::new(false),
    };
    let listeners = listen(&fixture, &context);
    let [a, b, c] = fixture.nodes;
    assert_eq!(
        divui_element_set_attribute(b, text("tabindex"), text("-1")),
        DivUiStatus::Ok
    );
    assert_eq!(divui_element_focus(b), DivUiStatus::Ok);
    let mut focused = ptr::null_mut();
    assert_eq!(
        divui_document_advance_focus(fixture.document, 1, &mut focused),
        DivUiStatus::Ok
    );
    assert_eq!(focused, c);
    assert_eq!(divui_element_focus(b), DivUiStatus::Ok);
    assert_eq!(
        divui_document_advance_focus(fixture.document, -1, &mut focused),
        DivUiStatus::Ok
    );
    assert_eq!(focused, a);
    assert_eq!(
        divui_document_advance_focus(fixture.document, 0, &mut focused),
        DivUiStatus::InvalidArgument
    );
    assert!(focused.is_null());
    assert!(fixture.native("a").has_focus().unwrap());
    assert_eq!(
        *context.events.borrow(),
        [
            (1, 12, Some("b".into())),
            (1, 13, None),
            (2, 12, Some("c".into())),
            (2, 13, None),
            (1, 12, Some("b".into())),
            (1, 13, None),
            (0, 12, Some("a".into())),
        ]
    );
    context.events.borrow_mut().clear();
    let mut modal = ptr::null_mut();
    assert_eq!(
        divui_element_create(fixture.document, 0, &mut modal),
        DivUiStatus::Ok
    );
    assert_eq!(
        divui_element_append_child(fixture.root, modal),
        DivUiStatus::Ok
    );
    fixture.native("c").detach().unwrap();
    assert_eq!(divui_element_append_child(modal, c), DivUiStatus::Ok);
    state.native.dispatch_composition_start().unwrap();
    state.native.dispatch_composition_update("preview").unwrap();
    assert_eq!(
        divui_document_set_modal_root(fixture.document, modal),
        DivUiStatus::Ok
    );
    assert!(fixture.native("c").has_focus().unwrap());
    assert_eq!(
        fixture.native("a").control_value().unwrap().as_deref(),
        Some("")
    );
    assert_eq!(
        divui_document_set_modal_root(fixture.document, ptr::null_mut()),
        DivUiStatus::Ok
    );
    assert_eq!(
        divui_document_set_modal_root(fixture.document, ptr::null_mut()),
        DivUiStatus::Ok
    );
    assert_eq!(
        *context.events.borrow(),
        [
            (0, 11, Some("a".into())),
            (0, 13, None),
            (2, 12, Some("c".into())),
            (2, 13, None),
            (0, 12, Some("a".into())),
        ]
    );
    assert!(!context.failed.get());
    for listener in listeners {
        assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
    }
    assert_eq!(divui_element_destroy(modal), DivUiStatus::Ok);
}

struct PhaseCallback {
    root: *mut DivUiElement,
    input: *mut DivUiElement,
    rows: RefCell<Vec<(u32, bool, u32, u32)>>,
    failed: Cell<bool>,
}

unsafe extern "C" fn observe_phase(event: *mut DivUiEvent, user_data: *mut c_void) {
    // SAFETY: the fixture retains both pointers through synchronous dispatch.
    let event = unsafe { &mut *event };
    let data = unsafe { &*user_data.cast::<PhaseCallback>() };
    data.rows.borrow_mut().push((
        event.event_type,
        event.current_target == data.root,
        event.phase,
        event.flags & DIVUI_EVENT_FLAG_DEFAULT_PREVENTED,
    ));
    if event.target != data.input
        || divui_element_set_attribute(data.input, text("data-phase"), text("seen"))
            != DivUiStatus::Ok
    {
        data.failed.set(true);
    }
    event.flags |= DIVUI_EVENT_FLAG_DEFAULT_PREVENTED;
}

#[test]
fn c_focus_requests_capture_without_bubbling_and_ignore_cancellation() {
    let fixture = Fixture::new();
    let input = fixture.nodes[0];
    let context = PhaseCallback {
        root: fixture.root,
        input,
        rows: RefCell::new(Vec::new()),
        failed: Cell::new(false),
    };
    let mut listeners = Vec::new();
    for node in [fixture.root, input] {
        for event in [12, 13] {
            for capture in [0, 1] {
                let mut listener = ptr::null_mut();
                assert_eq!(
                    divui_element_add_event_listener(
                        node,
                        event,
                        capture,
                        Some(observe_phase),
                        (&context as *const PhaseCallback).cast_mut().cast(),
                        &mut listener
                    ),
                    DivUiStatus::Ok
                );
                listeners.push(listener);
            }
        }
    }
    for kind in [12, 13] {
        let mut event = synthesized_event(kind, text(""));
        assert_eq!(
            divui_document_dispatch_event(fixture.document, input, &mut event),
            DivUiStatus::Ok
        );
        assert_eq!(event.current_target, ptr::null_mut());
        assert_eq!(event.flags & DIVUI_EVENT_FLAG_DEFAULT_PREVENTED, 0);
        assert_eq!(fixture.native("a").has_focus().unwrap(), kind == 12);
    }
    assert_eq!(
        *context.rows.borrow(),
        [
            (12, true, 1, 0),
            (12, false, 2, 0),
            (12, false, 2, 0),
            (13, true, 1, 0),
            (13, false, 2, 0),
            (13, false, 2, 0),
        ]
    );
    assert!(!context.failed.get());
    for listener in listeners {
        assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
    }
}

#[test]
fn c_pointer_focus_cancels_the_shared_composition_and_delivers_focus_callbacks() {
    let fixture = Fixture::new();
    for id in ["a", "b", "c"] {
        let input = fixture.native(id);
        input.set_display(div_style::Display::Block).unwrap();
        input.set_width(LengthValue::px(100.0)).unwrap();
        input.set_height(LengthValue::px(24.0)).unwrap();
    }
    let state = document(fixture.document as usize).unwrap();
    let context = Callback {
        document: fixture.document,
        nodes: fixture.nodes,
        events: RefCell::new(Vec::new()),
        redirect: Cell::new(false),
        failed: Cell::new(false),
    };
    let listeners = listen(&fixture, &context);
    assert_eq!(divui_element_focus(fixture.nodes[0]), DivUiStatus::Ok);
    state.native.dispatch_composition_start().unwrap();
    state.native.dispatch_composition_update("preview").unwrap();
    let bounds = fixture.native("b").bounding_rect().unwrap().unwrap();
    assert!(bounds.width > 0.0 && bounds.height > 0.0);
    let mut event = synthesized_event(1, text(""));
    event.x = bounds.x + bounds.width * 0.5;
    event.y = bounds.y + bounds.height * 0.5;
    event.pointer_id = 7;
    assert_eq!(
        divui_document_dispatch_pointer_event(fixture.document, &mut event),
        DivUiStatus::Ok
    );
    assert_eq!(event.target, fixture.nodes[1]);
    assert!(fixture.native("b").has_focus().unwrap());
    assert_eq!(
        fixture.native("a").control_value().unwrap().as_deref(),
        Some("")
    );
    assert_eq!(
        *context.events.borrow(),
        [
            (0, 12, Some("a".into())),
            (0, 11, Some("a".into())),
            (0, 13, None),
            (1, 12, Some("b".into())),
        ]
    );
    assert!(!context.failed.get());
    for listener in listeners {
        assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
    }
}
