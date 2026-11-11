use super::*;

fn text(value: &str) -> DivUiUtf8 {
    DivUiUtf8 {
        data: value.as_ptr(),
        length: value.len(),
    }
}

fn create_app() -> *mut DivUiApp {
    let config = DivUiAppConfig {
        struct_size: size_of::<DivUiAppConfig>() as u32,
        abi_version: DIVUI_ABI_VERSION,
        title: text("Native C"),
        width: 160,
        height: 100,
        backend: 2,
        reserved: 0,
    };
    let mut app = ptr::null_mut();
    assert_eq!(divui_app_create(&config, &mut app), DivUiStatus::Ok);
    app
}

fn run_config() -> DivUiAppRunConfig {
    DivUiAppRunConfig {
        struct_size: size_of::<DivUiAppRunConfig>() as u32,
        abi_version: DIVUI_ABI_VERSION,
        flags: 0,
        reserved: 0,
        callback: None,
        user_data: ptr::null_mut(),
    }
}

#[test]
fn native_run_config_and_lifecycle_reject_invalid_ownership() {
    assert_eq!(size_of::<DivUiAppRunConfig>(), 32);
    assert_eq!(size_of::<DivUiPlatformEvent>(), 152);
    let app = create_app();
    let short_header = [8_u32, DIVUI_ABI_VERSION];
    assert_eq!(
        divui_app_run(app, short_header.as_ptr().cast()),
        DivUiStatus::InvalidArgument
    );
    let short_size = 4_u32;
    assert_eq!(
        divui_app_run(app, (&short_size as *const u32).cast()),
        DivUiStatus::InvalidArgument
    );
    let mut config = run_config();
    config.reserved = 1;
    assert_eq!(divui_app_run(app, &config), DivUiStatus::InvalidArgument);
    config.reserved = 0;
    config.abi_version = 1;
    assert_eq!(divui_app_run(app, &config), DivUiStatus::AbiMismatch);
    config.abi_version = DIVUI_ABI_VERSION;
    config.struct_size = 8;
    assert_eq!(divui_app_run(app, &config), DivUiStatus::InvalidArgument);
    config.struct_size = size_of::<DivUiAppRunConfig>() as u32;
    let state = native_app::app_state(app).unwrap();
    state.running.set(true);
    assert_eq!(divui_app_run(app, &config), DivUiStatus::Reentrant);
    assert_eq!(divui_app_destroy(app), DivUiStatus::InvalidState);
    state.running.set(false);
    state.has_run.set(true);
    assert_eq!(divui_app_run(app, &config), DivUiStatus::InvalidState);
    let address = app as usize;
    let wrong_thread = std::thread::spawn(move || divui_app_request_exit(address as *mut DivUiApp))
        .join()
        .unwrap();
    assert_eq!(wrong_thread, DivUiStatus::WrongThread);
    assert_eq!(divui_app_request_exit(app), DivUiStatus::Ok);
    assert!(state.exit_requested.get());
    assert_eq!(divui_app_destroy(app), DivUiStatus::Ok);
    assert_eq!(divui_app_request_exit(app), DivUiStatus::InvalidHandle);
}

#[cfg(not(any(
    all(feature = "linux", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
)))]
#[test]
fn headless_c_build_preserves_app_api_and_reports_native_run_unavailable() {
    let app = create_app();
    assert_eq!(divui_app_run(app, &run_config()), DivUiStatus::InvalidState);
    assert!(!native_app::app_state(app).unwrap().has_run.get());
    assert_eq!(divui_app_destroy(app), DivUiStatus::Ok);
}

#[cfg(any(
    all(feature = "linux", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
mod linux {
    use super::*;
    use div_engine::FocusOrigin;
    use div_platform::{KeyPhase, KeyboardInput, Modifiers, PlatformApplication, PlatformEvent};

    fn document_and_input(
        app: *mut DivUiApp,
    ) -> (*mut DivUiDocument, *mut DivUiElement, *mut DivUiElement) {
        let mut document = ptr::null_mut();
        let mut root = ptr::null_mut();
        let mut input = ptr::null_mut();
        assert_eq!(divui_app_document(app, &mut document), DivUiStatus::Ok);
        assert_eq!(divui_document_root(document, &mut root), DivUiStatus::Ok);
        assert_eq!(
            divui_element_create(document, 23, &mut input),
            DivUiStatus::Ok
        );
        assert_eq!(divui_element_append_child(root, input), DivUiStatus::Ok);
        (document, root, input)
    }

    #[test]
    fn c_native_keyboard_uses_rust_editing_and_reentrant_cancellation() {
        unsafe extern "C" fn cancel(event: *mut DivUiEvent, user_data: *mut c_void) {
            // SAFETY: the test owns both values until synchronous dispatch ends.
            let event = unsafe { &mut *event };
            let input = user_data as *mut DivUiElement;
            assert_eq!(
                divui_element_set_control_value(input, text("callback")),
                DivUiStatus::Ok
            );
            event.flags |= DIVUI_EVENT_FLAG_DEFAULT_PREVENTED;
        }
        let app = create_app();
        let (document, root, input) = document_and_input(app);
        let state = native_app::app_state(app).unwrap();
        let node = element(input as usize).unwrap().node;
        borrow_engine_mut(&state.document)
            .unwrap()
            .focus_with_origin(node, FocusOrigin::Keyboard)
            .unwrap();
        let mut application = native_app::native_application(&state).unwrap();
        application
            .key_input(KeyboardInput {
                phase: KeyPhase::Down,
                key_code: 65,
                key_text: Some("a".into()),
                text: Some("a".into()),
                modifiers: Modifiers::default(),
                repeat: false,
            })
            .unwrap();
        assert_eq!(
            borrow_engine(&state.document)
                .unwrap()
                .control_state(node)
                .unwrap()
                .unwrap()
                .value,
            "a"
        );
        let mut listener = ptr::null_mut();
        assert_eq!(
            divui_element_add_event_listener(
                input,
                18,
                0,
                Some(cancel),
                input.cast(),
                &mut listener
            ),
            DivUiStatus::Ok
        );
        application
            .event(PlatformEvent::TextInput("blocked".into()))
            .unwrap();
        assert_eq!(
            borrow_engine(&state.document)
                .unwrap()
                .control_state(node)
                .unwrap()
                .unwrap()
                .value,
            "callback"
        );
        let frame = application.render(0.0).unwrap();
        let bitmap = state.document.native.render_to_bitmap().unwrap();
        assert_eq!(frame.pixels, bitmap.pixels());
        assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
        assert_eq!(
            divui_element_add_event_listener(
                input,
                6,
                0,
                Some(cancel),
                input.cast(),
                &mut listener
            ),
            DivUiStatus::Ok
        );
        application
            .key_input(KeyboardInput {
                phase: KeyPhase::Down,
                key_code: 66,
                key_text: Some("b".into()),
                text: Some("b".into()),
                modifiers: Modifiers::default(),
                repeat: false,
            })
            .unwrap();
        assert_eq!(
            borrow_engine(&state.document)
                .unwrap()
                .control_state(node)
                .unwrap()
                .unwrap()
                .value,
            "callback"
        );
        assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
        assert_eq!(divui_element_destroy(input), DivUiStatus::Ok);
        assert_eq!(divui_element_destroy(root), DivUiStatus::Ok);
        assert_eq!(divui_document_destroy(document), DivUiStatus::Ok);
        assert_eq!(divui_app_destroy(app), DivUiStatus::Ok);
    }

    #[test]
    fn native_accessibility_actions_reach_c_controls_and_c_listeners() {
        unsafe extern "C" fn count(_event: *mut DivUiEvent, user_data: *mut c_void) {
            // SAFETY: the counter is live through this synchronous test.
            unsafe { *(user_data as *mut u32) += 1 };
        }
        let app = create_app();
        let (document, root, checkbox) = document_and_input(app);
        assert_eq!(
            divui_element_set_attribute(checkbox, text("type"), text("checkbox")),
            DivUiStatus::Ok
        );
        let mut changes = 0_u32;
        let mut listener = ptr::null_mut();
        assert_eq!(
            divui_element_add_event_listener(
                checkbox,
                17,
                0,
                Some(count),
                (&mut changes as *mut u32).cast(),
                &mut listener
            ),
            DivUiStatus::Ok
        );
        let state = native_app::app_state(app).unwrap();
        let node = element(checkbox as usize).unwrap().node;
        let id = borrow_engine(&state.document)
            .unwrap()
            .accessibility_node_id(node)
            .unwrap();
        let mut application = native_app::native_application(&state).unwrap();
        let tree_id = application.accessibility_update().unwrap().tree_id;
        application
            .accessibility_action(div_platform::ActionRequest {
                action: div_engine::AccessibilityPlatformAction::Click,
                target_tree: tree_id,
                target_node: id,
                data: None,
            })
            .unwrap();
        let mut flags = 0;
        assert_eq!(
            divui_element_get_control_flags(checkbox, &mut flags),
            DivUiStatus::Ok
        );
        assert_ne!(flags & DIVUI_CONTROL_CHECKED, 0);
        assert_eq!(changes, 1);
        assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
        assert_eq!(divui_element_destroy(checkbox), DivUiStatus::Ok);
        assert_eq!(divui_element_destroy(root), DivUiStatus::Ok);
        assert_eq!(divui_document_destroy(document), DivUiStatus::Ok);
        assert_eq!(divui_app_destroy(app), DivUiStatus::Ok);
    }
}
