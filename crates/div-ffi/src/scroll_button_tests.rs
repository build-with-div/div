use super::*;

fn state_output() -> DivUiScrollButtonStateV1 {
    DivUiScrollButtonStateV1 {
        struct_size: size_of::<DivUiScrollButtonStateV1>() as u32,
        abi_version: DIVUI_ABI_VERSION,
        direction: u32::MAX,
        availability: u32::MAX,
        enabled: 0,
        has_target: 0,
        reserved: [0; 6],
        current_x: -1.0,
        current_y: -1.0,
        target_x: -1.0,
        target_y: -1.0,
    }
}

fn fixture() -> (*mut DivUiDocument, *mut DivUiElement) {
    let config = DivUiDocumentConfig {
        struct_size: size_of::<DivUiDocumentConfig>() as u32,
        abi_version: DIVUI_ABI_VERSION,
        viewport: DivUiViewportMetrics {
            logical_width: 320.0,
            logical_height: 240.0,
            physical_width: 320,
            physical_height: 240,
            device_scale_factor: 1.0,
            authority: 1,
            reserved: 0,
        },
    };
    let mut doc = ptr::null_mut();
    let mut origin = ptr::null_mut();
    assert_eq!(divui_document_create(&config, &mut doc), DivUiStatus::Ok);
    assert_eq!(divui_element_create(doc, 0, &mut origin), DivUiStatus::Ok);
    let state = document(doc as usize).unwrap();
    let scroller = element(origin as usize).unwrap().node;
    {
        let mut engine = borrow_engine_mut(&state).unwrap();
        let content = engine.create_native_element(ElementTag::Div).unwrap();
        let root = engine.root();
        engine.append_child(root, scroller).unwrap();
        engine.append_child(scroller, content).unwrap();
        for (node, property, literal) in [
            (scroller, StyleProperty::Width, "80px"),
            (scroller, StyleProperty::Height, "80px"),
            (scroller, StyleProperty::Overflow, "hidden"),
            (scroller, StyleProperty::ScrollbarWidth, "none"),
            (content, StyleProperty::Width, "80px"),
            (content, StyleProperty::Height, "500px"),
        ] {
            engine
                .set_property(node, property, parse_literal(property, literal).unwrap())
                .unwrap();
        }
    }
    (doc, origin)
}

#[test]
fn scroll_button_aliases_preserve_direction_identity_and_origin_lifetime() {
    let (doc, origin) = fixture();
    let mut buttons = Vec::new();
    for direction in 0..8 {
        let mut button = ptr::null_mut();
        assert_eq!(
            divui_element_create_scroll_button_v1(origin, direction, &mut button),
            DivUiStatus::Ok
        );
        let mut value = state_output();
        assert_eq!(
            divui_scroll_button_get_state_v1(button, &mut value),
            DivUiStatus::Ok
        );
        assert_eq!(value.direction, direction);
        let mut alias = ptr::null_mut();
        assert_eq!(
            divui_element_get_scroll_button_v1(origin, direction, &mut alias),
            DivUiStatus::Ok
        );
        assert_ne!(alias, button);
        let mut same = 0;
        assert_eq!(
            divui_element_is_same_node_v1(button, alias, &mut same),
            DivUiStatus::Ok
        );
        assert_eq!(same, 1);
        assert_eq!(divui_element_destroy(alias), DivUiStatus::Ok);
        let mut related = ptr::null_mut();
        assert_eq!(
            divui_scroll_button_get_origin_v1(button, &mut related),
            DivUiStatus::Ok
        );
        assert_eq!(
            divui_element_is_same_node_v1(origin, related, &mut same),
            DivUiStatus::Ok
        );
        assert_eq!(same, 1);
        assert_eq!(divui_element_destroy(related), DivUiStatus::Ok);
        let mut collision = 1usize as *mut DivUiElement;
        assert_eq!(
            divui_element_create_scroll_button_v1(origin, direction, &mut collision),
            DivUiStatus::InvalidState
        );
        assert_eq!(collision, 1usize as *mut DivUiElement);
        buttons.push(button);
    }
    assert_eq!(
        divui_scroll_button_set_direction_v1(buttons[0], 2),
        DivUiStatus::InvalidState
    );
    let mut unchanged = state_output();
    assert_eq!(
        divui_scroll_button_get_state_v1(buttons[0], &mut unchanged),
        DivUiStatus::Ok
    );
    assert_eq!(unchanged.direction, 0);
    assert_eq!(divui_element_remove(origin), DivUiStatus::Ok);
    for button in buttons {
        let mut value = state_output();
        let before = value;
        assert_eq!(
            divui_scroll_button_get_state_v1(button, &mut value),
            DivUiStatus::StaleHandle
        );
        assert_eq!(value, before);
        assert_eq!(divui_element_destroy(button), DivUiStatus::Ok);
    }
    assert_eq!(divui_element_destroy(origin), DivUiStatus::Ok);
    assert_eq!(divui_document_destroy(doc), DivUiStatus::Ok);
}

#[test]
fn scroll_button_c_boundaries_validate_headers_threads_and_borrows_before_ownership() {
    use std::mem::align_of;
    assert_eq!(
        (
            size_of::<DivUiScrollButtonStateV1>(),
            align_of::<DivUiScrollButtonStateV1>()
        ),
        (56, 8)
    );
    let (doc, origin) = fixture();
    let state = document(doc as usize).unwrap();
    let mut button = 1usize as *mut DivUiElement;
    let engine = borrow_engine_mut(&state).unwrap();
    assert_eq!(
        divui_element_create_scroll_button_v1(origin, 2, &mut button),
        DivUiStatus::Reentrant
    );
    assert_eq!(button, 1usize as *mut DivUiElement);
    drop(engine);
    let handles = state.element_handles.borrow_mut();
    assert_eq!(
        divui_element_create_scroll_button_v1(origin, 2, &mut button),
        DivUiStatus::Reentrant
    );
    assert_eq!(button, 1usize as *mut DivUiElement);
    drop(handles);
    let mut absent = 1usize as *mut DivUiElement;
    assert_eq!(
        divui_element_get_scroll_button_v1(origin, 2, &mut absent),
        DivUiStatus::Ok
    );
    assert!(absent.is_null());
    assert_eq!(
        divui_element_create_scroll_button_v1(origin, 99, &mut button),
        DivUiStatus::InvalidArgument
    );
    assert_eq!(button, 1usize as *mut DivUiElement);
    assert_eq!(
        divui_element_create_scroll_button_v1(origin, 2, ptr::null_mut()),
        DivUiStatus::InvalidArgument
    );
    assert_eq!(
        divui_element_create_scroll_button_v1(origin, 2, &mut button),
        DivUiStatus::Ok
    );
    let short = 4u32;
    assert_eq!(
        divui_scroll_button_get_state_v1(button, (&short as *const u32).cast_mut().cast()),
        DivUiStatus::InvalidArgument
    );
    let prefix = [8u32, DIVUI_ABI_VERSION];
    assert_eq!(
        divui_scroll_button_get_state_v1(button, prefix.as_ptr().cast_mut().cast()),
        DivUiStatus::InvalidArgument
    );
    let mut bad = state_output();
    bad.abi_version += 1;
    let unchanged = bad;
    assert_eq!(
        divui_scroll_button_get_state_v1(button, &mut bad),
        DivUiStatus::AbiMismatch
    );
    assert_eq!(bad, unchanged);
    #[repr(C)]
    struct Extended {
        value: DivUiScrollButtonStateV1,
        trailing: [u8; 16],
    }
    let mut extended = Extended {
        value: state_output(),
        trailing: [0x6d; 16],
    };
    extended.value.struct_size = size_of::<Extended>() as u32;
    assert_eq!(
        divui_scroll_button_get_state_v1(button, &mut extended.value),
        DivUiStatus::Ok
    );
    assert_eq!(extended.trailing, [0x6d; 16]);
    assert_eq!(extended.value.struct_size, size_of::<Extended>() as u32);
    let address = button as usize;
    assert_eq!(
        std::thread::spawn(move || {
            let mut value = state_output();
            divui_scroll_button_get_state_v1(address as *mut DivUiElement, &mut value)
        })
        .join()
        .unwrap(),
        DivUiStatus::WrongThread
    );
    let engine = borrow_engine_mut(&state).unwrap();
    let mut out = state_output();
    let before = out;
    assert_eq!(
        divui_scroll_button_get_state_v1(button, &mut out),
        DivUiStatus::Reentrant
    );
    assert_eq!(out, before);
    drop(engine);
    let handles = state.element_handles.borrow_mut();
    assert_eq!(
        divui_scroll_button_get_origin_v1(button, &mut absent),
        DivUiStatus::Reentrant
    );
    assert!(absent.is_null());
    drop(handles);
    assert_eq!(
        divui_scroll_button_get_snap_target_v1(button, 99, 0, &mut absent),
        DivUiStatus::InvalidArgument
    );
    assert_eq!(
        divui_scroll_button_get_snap_target_v1(button, 0, 99, &mut absent),
        DivUiStatus::InvalidArgument
    );
    assert!(absent.is_null());
    assert_eq!(
        divui_element_set_native_appearance_v1(button, 2),
        DivUiStatus::InvalidArgument
    );
    assert_eq!(
        divui_element_set_scroll_snap_strictness_v1(origin, 99),
        DivUiStatus::InvalidArgument
    );
    assert_eq!(
        divui_scroll_button_activate_v1(origin),
        DivUiStatus::InvalidState
    );
    assert_eq!(divui_element_destroy(button), DivUiStatus::Ok);
    assert_eq!(divui_element_destroy(origin), DivUiStatus::Ok);
    assert_eq!(divui_document_destroy(doc), DivUiStatus::Ok);
}

struct Callback {
    origin: *mut DivUiElement,
    button: *mut DivUiElement,
    cancel: Cell<bool>,
    clicks: Cell<u32>,
    changes: Cell<u32>,
    failures: RefCell<Vec<String>>,
}

unsafe extern "C" fn clicked(event: *mut DivUiEvent, context: *mut c_void) {
    // SAFETY: this owning-thread synchronous callback retains both values
    // until return. Record failures without unwinding across the C boundary.
    let context = unsafe { &*context.cast::<Callback>() };
    let event = unsafe { &mut *event };
    if event.target != context.origin || event.current_target != context.origin {
        context
            .failures
            .borrow_mut()
            .push("projected event identity".into());
    }
    if event.event_type == 4 {
        context.clicks.set(context.clicks.get() + 1);
        let mut snapshot = state_output();
        if divui_scroll_button_get_state_v1(context.button, &mut snapshot) != DivUiStatus::Ok {
            context
                .failures
                .borrow_mut()
                .push("callback retained Engine access".into());
        }
        if context.cancel.get() {
            event.flags |= DIVUI_EVENT_FLAG_DEFAULT_PREVENTED;
        }
    } else {
        context.changes.set(context.changes.get() + 1);
    }
}

#[test]
fn scroll_button_c_activation_shares_cancelable_origin_events_and_focus_projection() {
    let (doc, origin) = fixture();
    let mut button = ptr::null_mut();
    assert_eq!(
        divui_element_create_scroll_button_v1(origin, 2, &mut button),
        DivUiStatus::Ok
    );
    let mut context = Callback {
        origin,
        button,
        cancel: Cell::new(true),
        clicks: Cell::new(0),
        changes: Cell::new(0),
        failures: RefCell::new(Vec::new()),
    };
    let mut listeners = Vec::new();
    for kind in [4, 16, 17] {
        let mut listener = ptr::null_mut();
        assert_eq!(
            divui_element_add_event_listener(
                origin,
                kind,
                0,
                Some(clicked),
                (&mut context as *mut Callback).cast(),
                &mut listener
            ),
            DivUiStatus::Ok
        );
        listeners.push(listener);
    }
    assert_eq!(divui_scroll_button_activate_v1(button), DivUiStatus::Ok);
    let mut snapshot = state_output();
    assert_eq!(
        divui_scroll_button_get_state_v1(button, &mut snapshot),
        DivUiStatus::Ok
    );
    assert_eq!(snapshot.current_y, 0.0);
    context.cancel.set(false);
    assert_eq!(divui_scroll_button_activate_v1(button), DivUiStatus::Ok);
    assert_eq!(
        divui_scroll_button_get_state_v1(button, &mut snapshot),
        DivUiStatus::Ok
    );
    assert_eq!(snapshot.current_y, 70.0);
    assert_eq!(context.clicks.get(), 2);
    assert_eq!(context.changes.get(), 0);
    assert!(
        context.failures.borrow().is_empty(),
        "{:?}",
        context.failures.borrow()
    );
    let mut focused = ptr::null_mut();
    assert_eq!(
        divui_document_focused_element_v1(doc, &mut focused),
        DivUiStatus::Ok
    );
    assert!(!focused.is_null());
    let mut same = 0;
    assert_eq!(
        divui_element_is_same_node_v1(origin, focused, &mut same),
        DivUiStatus::Ok
    );
    assert_eq!(same, 1);
    assert_eq!(divui_element_destroy(focused), DivUiStatus::Ok);
    let copied = snapshot;
    for listener in listeners {
        assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
    }
    assert_eq!(divui_element_destroy(button), DivUiStatus::Ok);
    assert_eq!(divui_element_destroy(origin), DivUiStatus::Ok);
    assert_eq!(divui_document_destroy(doc), DivUiStatus::Ok);
    assert_eq!(copied.current_y, 70.0);
}
