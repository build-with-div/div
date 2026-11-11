//! Shared C removal transport. Callback failures are checked after return.
use super::*;
use std::mem::MaybeUninit;

// Existing DivUiEventType header value and shared dispatch_event_to click route.
const CLICK: u32 = 4;

fn text(value: &str) -> DivUiUtf8 {
    DivUiUtf8 {
        data: value.as_ptr(),
        length: value.len(),
    }
}

fn fixture() -> (*mut DivUiDocument, *mut DivUiElement, *mut DivUiElement) {
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
    let mut root = ptr::null_mut();
    let mut node = ptr::null_mut();
    assert_eq!(divui_document_create(&config, &mut doc), DivUiStatus::Ok);
    assert_eq!(divui_document_root(doc, &mut root), DivUiStatus::Ok);
    assert_eq!(divui_element_create(doc, 0, &mut node), DivUiStatus::Ok);
    assert_eq!(divui_element_append_child(root, node), DivUiStatus::Ok);
    (doc, root, node)
}

fn property(node: *mut DivUiElement, id: StyleProperty, literal: &str) {
    let mut value = MaybeUninit::<DivUiStyleValue>::uninit();
    assert_eq!(
        divui_style_value_parse(id as i32, text(literal), value.as_mut_ptr()),
        DivUiStatus::Ok
    );
    // SAFETY: the successful parser initialized the entire tagged value.
    let value = unsafe { value.assume_init() };
    assert_eq!(
        divui_element_set_property(node, id as i32, &value),
        DivUiStatus::Ok
    );
    if value.tag == 6 {
        // SAFETY: tag 6 selects the owned compound member.
        assert_eq!(
            divui_style_compound_destroy(unsafe { value.data.compound }.cast_mut()),
            DivUiStatus::Ok
        );
    }
}

fn remove(node: *mut DivUiElement, id: StyleProperty, expected: u8) {
    let mut output = 99;
    assert_eq!(
        divui_element_remove_property(node, id as i32, &mut output),
        DivUiStatus::Ok
    );
    assert_eq!(output, expected);
}

fn finish(doc: *mut DivUiDocument, root: *mut DivUiElement, node: *mut DivUiElement) {
    assert_eq!(divui_element_destroy(node), DivUiStatus::Ok);
    assert_eq!(divui_element_destroy(root), DivUiStatus::Ok);
    assert_eq!(divui_document_destroy(doc), DivUiStatus::Ok);
}

#[test]
fn c_removal_preserves_surviving_raw_shorthand_context_and_missing_is_idle() {
    let (doc, root, node) = fixture();
    property(root, StyleProperty::FontSize, "10px");
    property(node, StyleProperty::Padding, "2em");
    property(node, StyleProperty::PaddingLeft, "7px");
    remove(node, StyleProperty::PaddingLeft, 1);
    property(root, StyleProperty::FontSize, "20px");
    let state = document(doc as usize).unwrap();
    let handle = element(node as usize).unwrap().node;
    {
        let mut e = borrow_engine_mut(&state).unwrap();
        let s = e.computed_style(handle).unwrap();
        assert_eq!((s.padding_top.value(), s.padding_left.value()), (40.0, 0.0));
        e.scene().unwrap();
    }
    let stats = borrow_engine(&state).unwrap().stats();
    let dirty = borrow_engine(&state).unwrap().dirty_generations();
    remove(node, StyleProperty::PaddingLeft, 0);
    assert_eq!(borrow_engine(&state).unwrap().stats(), stats);
    assert_eq!(borrow_engine(&state).unwrap().dirty_generations(), dirty);
    remove(node, StyleProperty::Padding, 1);
    assert_eq!(
        borrow_engine(&state)
            .unwrap()
            .computed_style(handle)
            .unwrap()
            .padding_top
            .value(),
        0.0
    );
    drop(state);
    finish(doc, root, node);
}

#[test]
fn c_removal_reveals_constructor_inheritance_and_keeps_effect_mask_until_cancel() {
    let (doc, root, node) = fixture();
    property(root, StyleProperty::Color, "red");
    property(node, StyleProperty::Color, "blue");
    property(node, StyleProperty::BackgroundColor, "currentcolor");
    property(node, StyleProperty::Display, "none");
    remove(node, StyleProperty::Display, 1);
    remove(node, StyleProperty::Color, 1);
    let state = document(doc as usize).unwrap();
    let handle = element(node as usize).unwrap().node;
    {
        let e = borrow_engine(&state).unwrap();
        let s = e.computed_style(handle).unwrap();
        assert_eq!(s.display, div_style::Display::Block);
        assert_eq!((s.color, s.background_color), (Color::RED, Color::RED));
    }
    property(node, StyleProperty::Width, "120px");
    let id = borrow_engine_mut(&state)
        .unwrap()
        .animate(
            handle,
            PropertyKeyframes::typed(
                StyleProperty::Width,
                Keyframes::from_values(LengthValue::px(40.0), LengthValue::px(40.0)),
            )
            .unwrap(),
            AnimationOptions {
                duration_ms: 100.0,
                fill: FillMode::Both,
                ..AnimationOptions::default()
            },
            AnimationTimeline::Document,
        )
        .unwrap();
    remove(node, StyleProperty::Width, 1);
    let mut rect = MaybeUninit::<DivUiRect>::uninit();
    assert_eq!(
        divui_element_get_bounds(node, rect.as_mut_ptr()),
        DivUiStatus::Ok
    );
    assert_eq!(unsafe { rect.assume_init() }.width, 40.0);
    assert_eq!(divui_document_animation_cancel(doc, id.0), DivUiStatus::Ok);
    assert_eq!(
        divui_element_get_bounds(node, rect.as_mut_ptr()),
        DivUiStatus::Ok
    );
    assert_eq!(unsafe { rect.assume_init() }.width, 320.0);
    remove(node, StyleProperty::Width, 0);
    drop(state);
    finish(doc, root, node);
}

#[test]
fn c_removal_failures_preserve_output_and_source_including_reentrant_borrow() {
    let (doc, root, node) = fixture();
    property(node, StyleProperty::Width, "77px");
    let mut output = 99;
    assert_eq!(
        divui_element_remove_property(node, StyleProperty::Width as i32, ptr::null_mut()),
        DivUiStatus::InvalidArgument
    );
    assert_eq!(
        divui_element_remove_property(node, i32::MAX, &mut output),
        DivUiStatus::InvalidArgument
    );
    assert_eq!(output, 99);
    assert_eq!(
        divui_element_remove_property(doc.cast(), StyleProperty::Width as i32, &mut output),
        DivUiStatus::InvalidHandle
    );
    assert_eq!(output, 99);
    let state = document(doc as usize).unwrap();
    let handle = element(node as usize).unwrap().node;
    let borrow = borrow_engine(&state).unwrap();
    assert_eq!(
        divui_element_remove_property(node, StyleProperty::Width as i32, &mut output),
        DivUiStatus::Reentrant
    );
    assert_eq!(output, 99);
    assert_eq!(borrow.computed_style(handle).unwrap().width.value(), 77.0);
    drop(borrow);
    let address = node as usize;
    let (status, preserved) = std::thread::spawn(move || {
        let mut output = 99;
        let status = divui_element_remove_property(
            address as *mut DivUiElement,
            StyleProperty::Width as i32,
            &mut output,
        );
        (status, output)
    })
    .join()
    .unwrap();
    assert_eq!((status, preserved), (DivUiStatus::WrongThread, 99));
    remove(node, StyleProperty::Width, 1);
    drop(state);
    finish(doc, root, node);
}

#[test]
fn c_removal_rejects_stale_nodes_and_dead_handles_without_writing() {
    let (doc, root, node) = fixture();
    let state = document(doc as usize).unwrap();
    let handle = element(node as usize).unwrap().node;
    borrow_engine_mut(&state).unwrap().remove(handle).unwrap();
    let mut output = 99;
    assert_eq!(
        divui_element_remove_property(node, StyleProperty::Width as i32, &mut output),
        DivUiStatus::StaleHandle
    );
    assert_eq!(output, 99);
    drop(state);
    assert_eq!(divui_element_destroy(node), DivUiStatus::Ok);
    assert_eq!(
        divui_element_remove_property(node, StyleProperty::Width as i32, &mut output),
        DivUiStatus::InvalidHandle
    );
    assert_eq!(output, 99);
    assert_eq!(divui_document_destroy(doc), DivUiStatus::Ok);
    assert_eq!(
        divui_element_remove_property(root, StyleProperty::Display as i32, &mut output),
        DivUiStatus::InvalidHandle
    );
    assert_eq!(output, 99);
    assert_eq!(divui_element_destroy(root), DivUiStatus::Ok);
}

struct CallbackState {
    node: *mut DivUiElement,
    calls: usize,
    first: DivUiStatus,
    second: DivUiStatus,
    bounds: DivUiStatus,
    removed: [u8; 2],
    width: f32,
}

unsafe extern "C" fn remove_from_callback(_event: *mut DivUiEvent, data: *mut c_void) {
    // SAFETY: the caller-owned state is exclusively available for this invocation.
    let state = unsafe { &mut *data.cast::<CallbackState>() };
    state.calls += 1;
    state.first = divui_element_remove_property(
        state.node,
        StyleProperty::Width as i32,
        &mut state.removed[0],
    );
    state.second = divui_element_remove_property(
        state.node,
        StyleProperty::Width as i32,
        &mut state.removed[1],
    );
    let mut rect = MaybeUninit::<DivUiRect>::uninit();
    state.bounds = divui_element_get_bounds(state.node, rect.as_mut_ptr());
    if state.bounds == DivUiStatus::Ok {
        state.width = unsafe { rect.assume_init() }.width;
    }
}

#[test]
fn c_callback_removal_uses_same_live_route_and_releases_all_engine_borrows() {
    let (doc, root, node) = fixture();
    property(node, StyleProperty::Width, "77px");
    let mut state = CallbackState {
        node,
        calls: 0,
        first: DivUiStatus::Internal,
        second: DivUiStatus::Internal,
        bounds: DivUiStatus::Internal,
        removed: [99, 99],
        width: -1.0,
    };
    let mut listener = ptr::null_mut();
    assert_eq!(
        divui_element_add_event_listener(
            node,
            CLICK as i32,
            0,
            Some(remove_from_callback),
            (&mut state as *mut CallbackState).cast(),
            &mut listener,
        ),
        DivUiStatus::Ok
    );
    let mut event = synthesized_event(CLICK, text(""));
    assert_eq!(
        divui_document_dispatch_event(doc, node, &mut event),
        DivUiStatus::Ok
    );
    assert_eq!(state.calls, 1);
    assert_eq!(
        (state.first, state.second, state.bounds),
        (DivUiStatus::Ok, DivUiStatus::Ok, DivUiStatus::Ok)
    );
    assert_eq!(state.removed, [1, 0]);
    assert_eq!(state.width, 320.0);
    assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
    finish(doc, root, node);
}
