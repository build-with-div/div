//! Consuming C guards for shared native registration and callback ownership.
//! Callback failures are recorded and checked after returning from extern C.

use super::*;

fn fixture() -> (*mut DivUiDocument, *mut DivUiElement, Rc<DocumentState>) {
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
    let mut node = ptr::null_mut();
    assert_eq!(divui_document_create(&config, &mut doc), DivUiStatus::Ok);
    assert_eq!(divui_element_create(doc, 0, &mut node), DivUiStatus::Ok);
    let state = document(doc as usize).unwrap();
    let handle = element(node as usize).unwrap().node;
    let mut engine = borrow_engine_mut(&state).unwrap();
    let root = engine.root();
    engine.append_child(root, handle).unwrap();
    drop(engine);
    (doc, node, state)
}

struct Context {
    doc: *mut DivUiDocument,
    node: *mut DivUiElement,
    own: Cell<*mut DivUiListener>,
    victim: Cell<*mut DivUiListener>,
    replacement: Cell<*mut DivUiListener>,
    changed: Cell<bool>,
    nested: bool,
    log: Rc<RefCell<Vec<u8>>>,
    statuses: RefCell<Vec<DivUiStatus>>,
}

impl Context {
    fn new(doc: *mut DivUiDocument, node: *mut DivUiElement, nested: bool) -> Self {
        Self {
            doc,
            node,
            own: Cell::new(ptr::null_mut()),
            victim: Cell::new(ptr::null_mut()),
            replacement: Cell::new(ptr::null_mut()),
            changed: Cell::new(false),
            nested,
            log: Rc::new(RefCell::new(Vec::new())),
            statuses: RefCell::new(Vec::new()),
        }
    }
}

fn click(doc: *mut DivUiDocument, node: *mut DivUiElement) -> DivUiStatus {
    let mut event = synthesized_event(
        4,
        DivUiUtf8 {
            data: ptr::null(),
            length: 0,
        },
    );
    divui_document_dispatch_event(doc, node, &mut event)
}

unsafe extern "C" fn victim(_: *mut DivUiEvent, data: *mut c_void) {
    // SAFETY: context is kept alive through the synchronous dispatch.
    let context = unsafe { &*data.cast::<Context>() };
    context.log.borrow_mut().push(2);
}

unsafe extern "C" fn replacement(_: *mut DivUiEvent, data: *mut c_void) {
    // SAFETY: context is kept alive through the synchronous dispatch.
    let context = unsafe { &*data.cast::<Context>() };
    context.log.borrow_mut().push(3);
}

unsafe extern "C" fn remove_readd_nested(_: *mut DivUiEvent, data: *mut c_void) {
    // SAFETY: context is kept alive through this call and nested synchronous calls.
    let context = unsafe { &*data.cast::<Context>() };
    context.log.borrow_mut().push(1);
    if context.changed.replace(true) {
        return;
    }
    let status = divui_listener_destroy(context.victim.get());
    context.statuses.borrow_mut().push(status);
    let mut added = ptr::null_mut();
    let status =
        divui_element_add_event_listener(context.node, 4, 0, Some(replacement), data, &mut added);
    context.statuses.borrow_mut().push(status);
    context.replacement.set(added);
    if context.nested {
        let status = click(context.doc, context.node);
        context.statuses.borrow_mut().push(status);
    }
}

unsafe extern "C" fn destroy_self(_: *mut DivUiEvent, data: *mut c_void) {
    // SAFETY: self-destruction does not end this active callback's data lifetime.
    let context = unsafe { &*data.cast::<Context>() };
    context.log.borrow_mut().push(1);
    let status = divui_listener_destroy(context.own.get());
    context.statuses.borrow_mut().push(status);
}

unsafe extern "C" fn stop_immediately(event: *mut DivUiEvent, data: *mut c_void) {
    // SAFETY: the event and context live through this synchronous callback.
    let context = unsafe { &*data.cast::<Context>() };
    let event = unsafe { &mut *event };
    context.log.borrow_mut().push(2);
    event.flags |= DIVUI_EVENT_FLAG_IMMEDIATE_PROPAGATION_STOPPED;
}

fn add(
    node: *mut DivUiElement,
    callback: DivUiEventCallback,
    data: *mut c_void,
) -> *mut DivUiListener {
    let mut listener = ptr::null_mut();
    assert_eq!(
        divui_element_add_event_listener(node, 4, 0, Some(callback), data, &mut listener),
        DivUiStatus::Ok
    );
    listener
}

#[test]
fn removed_ids_are_skipped_readded_ids_wait_and_nested_dispatch_sees_current_ids() {
    for nested in [false, true] {
        let (doc, node, state) = fixture();
        let context = Context::new(doc, node, nested);
        let data = (&context as *const Context).cast_mut().cast();
        context.own.set(add(node, remove_readd_nested, data));
        context.victim.set(add(node, victim, data));
        assert_eq!(click(doc, node), DivUiStatus::Ok);
        assert_eq!(
            *context.log.borrow(),
            if nested { vec![1, 1, 3] } else { vec![1] }
        );
        assert!(context
            .statuses
            .borrow()
            .iter()
            .all(|status| *status == DivUiStatus::Ok));
        context.log.borrow_mut().clear();
        assert_eq!(click(doc, node), DivUiStatus::Ok);
        assert_eq!(*context.log.borrow(), vec![1, 3]);
        assert_eq!(divui_listener_destroy(context.own.get()), DivUiStatus::Ok);
        assert_eq!(
            divui_listener_destroy(context.replacement.get()),
            DivUiStatus::Ok
        );
        assert_eq!(
            divui_listener_destroy(context.victim.get()),
            DivUiStatus::InvalidHandle
        );
        assert!(state.listeners.borrow().is_empty());
        assert_eq!(divui_element_destroy(node), DivUiStatus::Ok);
        assert_eq!(divui_document_destroy(doc), DivUiStatus::Ok);
    }
}

#[test]
fn self_destruction_keeps_only_the_active_call_and_removes_future_callbacks() {
    let (doc, node, _) = fixture();
    let context = Context::new(doc, node, false);
    let data = (&context as *const Context).cast_mut().cast();
    context.own.set(add(node, destroy_self, data));
    assert_eq!(click(doc, node), DivUiStatus::Ok);
    assert_eq!(click(doc, node), DivUiStatus::Ok);
    assert_eq!(*context.log.borrow(), vec![1]);
    assert_eq!(*context.statuses.borrow(), vec![DivUiStatus::Ok]);
    assert_eq!(
        divui_listener_destroy(context.own.get()),
        DivUiStatus::InvalidHandle
    );
    assert_eq!(divui_element_destroy(node), DivUiStatus::Ok);
    assert_eq!(divui_document_destroy(doc), DivUiStatus::Ok);
}

#[test]
fn borrowed_listener_storage_preserves_add_outputs_destroy_tokens_and_aliases() {
    let (doc, node, state) = fixture();
    let context = Context::new(doc, node, false);
    let data = (&context as *const Context).cast_mut().cast();
    let listener = add(node, victim, data);
    let next_id = state.next_listener.get();
    let sentinel = 1usize as *mut DivUiListener;
    let mut output = sentinel;
    {
        let held = state.listeners.borrow();
        assert_eq!(
            divui_element_add_event_listener(node, 4, 0, Some(victim), data, &mut output),
            DivUiStatus::Reentrant
        );
        assert_eq!(output, sentinel);
        assert_eq!(state.next_listener.get(), next_id);
        assert_eq!(divui_listener_destroy(listener), DivUiStatus::Reentrant);
        assert_eq!(divui_element_destroy(node), DivUiStatus::Reentrant);
        assert_eq!(held.len(), 1);
    }
    assert!(get(listener as usize, HandleKind::Listener).is_ok());
    {
        let held = state.element_handles.borrow();
        assert_eq!(divui_element_destroy(node), DivUiStatus::Reentrant);
        assert_eq!(state.listeners.borrow().len(), 1);
        assert!(held
            .values()
            .any(|aliases| aliases.contains(&(node as usize))));
    }
    assert_eq!(click(doc, node), DivUiStatus::Ok);
    assert_eq!(*context.log.borrow(), vec![2]);
    assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
    assert_eq!(divui_element_destroy(node), DivUiStatus::Ok);
    assert_eq!(divui_document_destroy(doc), DivUiStatus::Ok);
}

#[test]
fn native_removal_invalidates_shared_c_route_callbacks() {
    let (doc, node, state) = fixture();
    let context = Context::new(doc, node, false);
    let data = (&context as *const Context).cast_mut().cast();
    let listener = add(node, victim, data);
    let handle = element(node as usize).unwrap().node;
    let native_id = state.listeners.borrow()[0].native_registration_id;
    assert!(state
        .native
        .remove_listener_for_native_facade(native_id)
        .unwrap());
    let mut raw = synthesized_event(
        4,
        DivUiUtf8 {
            data: ptr::null(),
            length: 0,
        },
    );
    dispatch_event_to(&state, handle, node as usize, &mut raw, false).unwrap();
    assert!(context.log.borrow().is_empty());
    assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
    assert_eq!(divui_element_destroy(node), DivUiStatus::Ok);
    assert_eq!(divui_document_destroy(doc), DivUiStatus::Ok);
}

#[test]
fn destroying_one_alias_removes_its_listeners_but_preserves_other_aliases() {
    let (doc, node, state) = fixture();
    let handle = element(node as usize).unwrap().node;
    let alias = focus_events::owned_element_handle(&state, handle).unwrap();
    let context = Context::new(doc, alias, false);
    let data = (&context as *const Context).cast_mut().cast();
    let listener = add(node, victim, data);
    assert_eq!(divui_element_destroy(node), DivUiStatus::Ok);
    assert_eq!(click(doc, alias), DivUiStatus::Ok);
    assert!(context.log.borrow().is_empty());
    assert!(state.listeners.borrow().is_empty());
    assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
    assert_eq!(divui_element_destroy(alias), DivUiStatus::Ok);
    assert_eq!(divui_document_destroy(doc), DivUiStatus::Ok);
}

#[test]
fn rust_and_c_registrations_follow_one_order_and_immediate_stop_state() {
    for stop in [false, true] {
        let (doc, node, state) = fixture();
        let handle = element(node as usize).unwrap().node;
        borrow_engine_mut(&state)
            .unwrap()
            .set_attribute(handle, "id", "listener-node")
            .unwrap();
        let native = state
            .native
            .element_by_id("listener-node")
            .unwrap()
            .unwrap();
        let context = Context::new(doc, node, false);
        let log = context.log.clone();
        native
            .on("click", move |_| log.borrow_mut().push(1))
            .unwrap();
        let data = (&context as *const Context).cast_mut().cast();
        let listener = add(node, if stop { stop_immediately } else { victim }, data);
        let log = context.log.clone();
        native
            .on("click", move |_| log.borrow_mut().push(3))
            .unwrap();
        assert_eq!(click(doc, node), DivUiStatus::Ok);
        assert_eq!(
            *context.log.borrow(),
            if stop { vec![1, 2] } else { vec![1, 2, 3] }
        );
        native.remove_event("click").unwrap();
        context.log.borrow_mut().clear();
        assert_eq!(click(doc, node), DivUiStatus::Ok);
        let mut raw = synthesized_event(
            4,
            DivUiUtf8 {
                data: ptr::null(),
                length: 0,
            },
        );
        dispatch_event_to(&state, handle, node as usize, &mut raw, false).unwrap();
        assert!(context.log.borrow().is_empty());
        assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
        assert_eq!(divui_element_destroy(node), DivUiStatus::Ok);
        assert_eq!(divui_document_destroy(doc), DivUiStatus::Ok);
    }
}

struct CaptureDropProbe {
    live_alias: *mut DivUiElement,
    statuses: Rc<RefCell<Vec<DivUiStatus>>>,
}

impl Drop for CaptureDropProbe {
    fn drop(&mut self) {
        // A successful lookup proves registry and alias/list ownership guards
        // were released before the last foreign capture destructor runs.
        let mut id = 0;
        let status = divui_element_get_accessibility_id(self.live_alias, &mut id);
        self.statuses.borrow_mut().push(status);
    }
}

#[test]
fn last_foreign_captures_drop_after_c_listener_alias_and_registry_guards() {
    for destroy_element in [false, true] {
        let (doc, node, state) = fixture();
        let handle = element(node as usize).unwrap().node;
        let alias = focus_events::owned_element_handle(&state, handle).unwrap();
        let context = Context::new(doc, node, false);
        let data = (&context as *const Context).cast_mut().cast();
        let listener = add(node, victim, data);
        let old_id = state.listeners.borrow()[0].native_registration_id;
        assert!(state
            .native
            .remove_listener_for_native_facade(old_id)
            .unwrap());
        let statuses = Rc::new(RefCell::new(Vec::new()));
        let probe = CaptureDropProbe {
            live_alias: alias,
            statuses: statuses.clone(),
        };
        let id = state
            .native
            .add_foreign_listener_for_native_facade(
                handle,
                "click",
                false,
                Rc::new(move |_, _, _, _| {
                    let _ = &probe;
                    Ok(())
                }),
            )
            .unwrap();
        state.listeners.borrow_mut()[0].native_registration_id = id;
        if destroy_element {
            assert_eq!(divui_element_destroy(node), DivUiStatus::Ok);
            assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
        } else {
            assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
            assert_eq!(divui_element_destroy(node), DivUiStatus::Ok);
        }
        assert_eq!(*statuses.borrow(), vec![DivUiStatus::Ok]);
        assert_eq!(divui_element_destroy(alias), DivUiStatus::Ok);
        assert_eq!(divui_document_destroy(doc), DivUiStatus::Ok);
    }
}
