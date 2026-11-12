//! Real retained Engine/C boundary guards; execution awaits integration qualification.
use super::*;
use std::mem::MaybeUninit;

const ZERO: DivUiTimelineLengthV1 = DivUiTimelineLengthV1 {
    value: 0.0,
    calc_offset: 0.0,
    unit: 0,
    reserved: 0,
};
fn offset(name: u32, unit: u32, value: f64, calc_offset: f64) -> DivUiTimelineRangeOffsetV1 {
    DivUiTimelineRangeOffsetV1 {
        name,
        reserved: 0,
        offset: DivUiTimelineLengthV1 {
            value,
            calc_offset,
            unit,
            reserved: 0,
        },
    }
}
fn range() -> DivUiAnimationAttachmentRangeV1 {
    DivUiAnimationAttachmentRangeV1 {
        struct_size: size_of::<DivUiAnimationAttachmentRangeV1>() as u32,
        abi_version: DIVUI_ABI_VERSION,
        present: 0,
        reserved: 0,
        start: offset(0, 0, 0.0, 0.0),
        end: offset(0, 0, 0.0, 0.0),
    }
}
fn source(node: *mut DivUiElement, kind: u32) -> DivUiAnimationTimelineSourceV1 {
    DivUiAnimationTimelineSourceV1 {
        struct_size: size_of::<DivUiAnimationTimelineSourceV1>() as u32,
        abi_version: DIVUI_ABI_VERSION,
        kind,
        axis: 0,
        source: node,
        inset_present: 0,
        reserved: 0,
        inset_start: ZERO,
        inset_end: ZERO,
    }
}
fn output<T>() -> T {
    // All new transport fields are numeric values/pointers, so all-zero is valid.
    let mut value = unsafe { MaybeUninit::<T>::zeroed().assume_init() };
    unsafe {
        ptr::write_unaligned((&mut value as *mut T).cast::<u32>(), size_of::<T>() as u32);
        ptr::write_unaligned(
            (&mut value as *mut T).cast::<u32>().add(1),
            DIVUI_ABI_VERSION,
        );
    }
    value
}
fn options() -> DivUiAnimationOptionsV1 {
    DivUiAnimationOptionsV1 {
        struct_size: size_of::<DivUiAnimationOptionsV1>() as u32,
        abi_version: DIVUI_ABI_VERSION,
        delay_ms: 0.0,
        duration_ms: 1000.0,
        iterations: 1.0,
        playback_rate: 1.0,
        direction: 0,
        fill: 3,
        play_state: 0,
        composite: 0,
        easing: ptr::null(),
    }
}
fn frame(offset: f64, number: f32) -> DivUiKeyframeV1 {
    DivUiKeyframeV1 {
        struct_size: size_of::<DivUiKeyframeV1>() as u32,
        abi_version: DIVUI_ABI_VERSION,
        offset,
        value: DivUiStyleValue {
            tag: 2,
            reserved: 0,
            data: DivUiStylePayload { number },
        },
        easing: ptr::null(),
    }
}
struct Context {
    document: *mut DivUiDocument,
    root: *mut DivUiElement,
    nodes: Vec<*mut DivUiElement>,
}
impl Context {
    fn new() -> Self {
        let mut document = ptr::null_mut();
        let mut root = ptr::null_mut();
        assert_eq!(
            divui_document_create(
                &DivUiDocumentConfig {
                    struct_size: size_of::<DivUiDocumentConfig>() as u32,
                    abi_version: DIVUI_ABI_VERSION,
                    viewport: DivUiViewportMetrics {
                        logical_width: 320.0,
                        logical_height: 240.0,
                        physical_width: 320,
                        physical_height: 240,
                        device_scale_factor: 1.0,
                        authority: 1,
                        reserved: 0
                    }
                },
                &mut document
            ),
            DivUiStatus::Ok
        );
        assert_eq!(divui_document_root(document, &mut root), DivUiStatus::Ok);
        Self {
            document,
            root,
            nodes: Vec::new(),
        }
    }
    fn node(&mut self, parent: *mut DivUiElement) -> *mut DivUiElement {
        let mut node = ptr::null_mut();
        assert_eq!(
            divui_element_create(self.document, 0, &mut node),
            DivUiStatus::Ok
        );
        assert_eq!(divui_element_append_child(parent, node), DivUiStatus::Ok);
        self.nodes.push(node);
        node
    }
    fn set(&self, node: *mut DivUiElement, property: StyleProperty, value: impl Into<StyleValue>) {
        let handle = element(node as usize).unwrap();
        let state = element_document(&handle).unwrap();
        borrow_engine_mut(&state)
            .unwrap()
            .set_property(handle.node, property, value.into())
            .unwrap();
    }
    fn port(&mut self) -> (*mut DivUiElement, *mut DivUiElement) {
        let port = self.node(self.root);
        self.set(port, StyleProperty::Width, LengthValue::px(100.0));
        self.set(port, StyleProperty::Height, LengthValue::px(80.0));
        self.set(port, StyleProperty::Overflow, div_style::Overflow::Hidden);
        let content = self.node(port);
        self.set(content, StyleProperty::Width, LengthValue::px(100.0));
        self.set(content, StyleProperty::Height, LengthValue::px(240.0));
        (port, content)
    }
    fn animate(
        &self,
        node: *mut DivUiElement,
        source: *const DivUiAnimationTimelineSourceV1,
        range: *const DivUiAnimationAttachmentRangeV1,
    ) -> u64 {
        let frames = [frame(0.0, 0.0), frame(1.0, 1.0)];
        let mut id = u64::MAX;
        assert_eq!(
            divui_element_animate_with_attachment_range_v1(
                node,
                StyleProperty::Opacity as i32,
                frames.as_ptr(),
                2,
                size_of::<DivUiKeyframeV1>(),
                &options(),
                source,
                range,
                &mut id
            ),
            DivUiStatus::Ok
        );
        id
    }
    fn count(&self) -> usize {
        let state = document(self.document as usize).unwrap();
        let n = borrow_engine(&state).unwrap().active_animation_count();
        n
    }
}
impl Drop for Context {
    fn drop(&mut self) {
        for node in self.nodes.drain(..) {
            assert_eq!(divui_element_destroy(node), DivUiStatus::Ok);
        }
        if !self.root.is_null() {
            assert_eq!(divui_element_destroy(self.root), DivUiStatus::Ok);
        }
        if !self.document.is_null() {
            assert_eq!(divui_document_destroy(self.document), DivUiStatus::Ok);
        }
    }
}

fn animation(context: &Context, id: u64) -> DivUiAnimationState {
    let mut state: DivUiAnimationState = output();
    assert_eq!(
        divui_document_animation_get_state(context.document, id, &mut state),
        DivUiStatus::Ok
    );
    state
}
fn percentage(context: &Context, id: u64) -> f64 {
    let mut value: DivUiAnimationTimelineSnapshotV1 = output();
    assert_eq!(
        divui_document_get_animation_timeline_snapshot_v1(context.document, id, &mut value),
        DivUiStatus::Ok
    );
    assert_eq!(divui_element_destroy(value.source), DivUiStatus::Ok);
    value.current_time_percentage
}
fn raw_bytes<T>(value: &T) -> Vec<u8> {
    // These guards use fully initialized repr(C) carriers with no uninitialized padding.
    unsafe { std::slice::from_raw_parts((value as *const T).cast(), size_of::<T>()).to_vec() }
}
#[test]
fn source_setter_copies_input_and_preserves_idle_identity_and_paused_document_hold() {
    let mut c = Context::new();
    let (a, _) = c.port();
    let (b, _) = c.port();
    let target = c.node(c.root);
    assert_eq!(divui_element_scroll_to(a, 0.0, 40.0), DivUiStatus::Ok);
    assert_eq!(divui_element_scroll_to(b, 0.0, 120.0), DivUiStatus::Ok);
    let id = c.animate(target, &source(a, 1), ptr::null());
    assert_eq!(
        divui_document_seek_animation_progress_v1(c.document, id, 50.0),
        DivUiStatus::Ok
    );
    let before = animation(&c, id);
    let owner = document(c.document as usize).unwrap();
    let generations = borrow_engine(&owner).unwrap().dirty_generations();
    let stats = borrow_engine(&owner).unwrap().stats();
    assert_eq!(
        divui_document_set_animation_timeline_source_v1(c.document, id, &source(a, 1)),
        DivUiStatus::Ok
    );
    assert_eq!(animation(&c, id).current_time_ms, before.current_time_ms);
    assert_eq!(
        borrow_engine(&owner).unwrap().dirty_generations(),
        generations
    );
    assert_eq!(borrow_engine(&owner).unwrap().stats(), stats);
    #[repr(C)]
    struct Future {
        known: DivUiAnimationTimelineSourceV1,
        tail: u64,
    }
    let mut future = Future {
        known: source(b, 1),
        tail: 0xcafe,
    };
    future.known.struct_size = size_of::<Future>() as u32;
    let bytes = raw_bytes(&future);
    assert_eq!(
        divui_document_set_animation_timeline_source_v1(c.document, id, &future.known),
        DivUiStatus::Ok
    );
    assert_eq!(raw_bytes(&future), bytes);
    assert_eq!(percentage(&c, id), 75.0);
    future.known.source = a;
    future.known.kind = 99;
    assert_eq!(percentage(&c, id), 75.0);
    assert_eq!(
        divui_document_animation_pause(c.document, id),
        DivUiStatus::Ok
    );
    assert_eq!(
        divui_document_set_animation_timeline_source_v1(c.document, id, ptr::null()),
        DivUiStatus::Ok
    );
    let result = animation(&c, id);
    assert_eq!(result.current_time_ms, 750.0);
    assert_eq!(result.play_state, 1);
    let mut progress: DivUiAnimationTimelineSnapshotV1 = output();
    assert_eq!(
        divui_document_get_animation_timeline_snapshot_v1(c.document, id, &mut progress),
        DivUiStatus::Ok
    );
    assert_eq!(progress.present, 0);
    assert!(progress.source.is_null());
    let mut attachment: DivUiAnimationAttachmentSnapshotV1 = output();
    assert_eq!(
        divui_document_get_animation_attachment_snapshot_v1(c.document, id, &mut attachment),
        DivUiStatus::Ok
    );
    assert_eq!(attachment.present, 1);
    assert!(attachment.source.is_null());
}
#[test]
fn rejected_source_metadata_leaves_the_effect_and_input_unchanged() {
    let mut c = Context::new();
    let (a, _) = c.port();
    let (b, _) = c.port();
    let target = c.node(c.root);
    let id = c.animate(target, &source(a, 1), ptr::null());
    let old = animation(&c, id);
    let owner = document(c.document as usize).unwrap();
    let generations = borrow_engine(&owner).unwrap().dirty_generations();
    let mut invalids = Vec::new();
    let mut v = source(b, 1);
    v.kind = 99;
    invalids.push(v);
    let mut v = source(b, 1);
    v.axis = 4;
    invalids.push(v);
    let mut v = source(b, 1);
    v.reserved = 1;
    invalids.push(v);
    let mut v = source(b, 1);
    v.inset_present = 1;
    invalids.push(v);
    let mut v = source(b, 2);
    v.inset_present = 1;
    v.inset_start.value = f64::MAX;
    invalids.push(v);
    let mut v = source(b, 2);
    v.inset_present = 1;
    v.inset_start.value = f64::NAN;
    invalids.push(v);
    for value in invalids {
        let before = raw_bytes(&value);
        assert_eq!(
            divui_document_set_animation_timeline_source_v1(c.document, id, &value),
            DivUiStatus::InvalidArgument
        );
        assert_eq!(raw_bytes(&value), before);
        assert_eq!(animation(&c, id).current_time_ms, old.current_time_ms);
        assert_eq!(
            borrow_engine(&owner).unwrap().dirty_generations(),
            generations
        );
    }
    let short = [8u32, DIVUI_ABI_VERSION];
    assert_eq!(
        divui_document_set_animation_timeline_source_v1(c.document, id, short.as_ptr().cast()),
        DivUiStatus::InvalidArgument
    );
    let mut v = source(b, 1);
    v.abi_version = 0;
    assert_eq!(
        divui_document_set_animation_timeline_source_v1(c.document, id, &v),
        DivUiStatus::AbiMismatch
    );
    assert_eq!(
        divui_document_set_animation_timeline_source_v1(c.document, u64::MAX, &source(b, 1)),
        DivUiStatus::InvalidState
    );
    assert_eq!(c.count(), 1);
}
#[test]
fn source_owner_thread_borrow_and_generation_checks_precede_publication() {
    let mut c = Context::new();
    let (a, _) = c.port();
    let (b, _) = c.port();
    let target = c.node(c.root);
    let id = c.animate(target, &source(a, 1), ptr::null());
    let mut foreign = Context::new();
    let (f, _) = foreign.port();
    assert_eq!(
        divui_document_set_animation_timeline_source_v1(c.document, id, &source(f, 1)),
        DivUiStatus::WrongDocument
    );
    let owner = document(c.document as usize).unwrap();
    {
        let _borrow = borrow_engine_mut(&owner).unwrap();
        assert_eq!(
            divui_document_set_animation_timeline_source_v1(c.document, id, &source(b, 1)),
            DivUiStatus::Reentrant
        );
    }
    let doc = c.document as usize;
    let address = b as usize;
    assert_eq!(
        std::thread::spawn(move || divui_document_set_animation_timeline_source_v1(
            doc as *mut DivUiDocument,
            id,
            &source(address as *mut DivUiElement, 1)
        ))
        .join()
        .unwrap(),
        DivUiStatus::WrongThread
    );
    assert_eq!(divui_element_remove(a), DivUiStatus::Ok);
    assert_eq!(
        divui_document_set_animation_timeline_source_v1(c.document, id, &source(a, 1)),
        DivUiStatus::StaleHandle
    );
    assert_eq!(
        divui_document_set_animation_timeline_source_v1(c.document, id, &source(b, 1)),
        DivUiStatus::Ok
    );
    let dead = c.document;
    c.document = ptr::null_mut();
    assert_eq!(divui_document_destroy(dead), DivUiStatus::Ok);
    assert_eq!(
        divui_document_set_animation_timeline_source_v1(dead, id, ptr::null()),
        DivUiStatus::InvalidHandle
    );
    // Destroy aliases after their owning document dies; there is no retained callback or input pointer.
    for n in c.nodes.drain(..) {
        assert_eq!(divui_element_destroy(n), DivUiStatus::Ok);
    }
    assert_eq!(divui_element_destroy(c.root), DivUiStatus::Ok);
    c.root = ptr::null_mut();
    drop(c);
}
struct Reentry {
    document: *mut DivUiDocument,
    id: Cell<u64>,
    calls: Cell<u32>,
    set_status: Cell<DivUiStatus>,
    pause_status: Cell<DivUiStatus>,
}
extern "C" fn rebind_from_start(_: *mut DivUiEvent, data: *mut c_void) {
    let c = unsafe { &*data.cast::<Reentry>() };
    c.calls.set(c.calls.get() + 1);
    c.set_status
        .set(divui_document_set_animation_timeline_source_v1(
            c.document,
            c.id.get(),
            ptr::null(),
        ));
    c.pause_status
        .set(divui_document_animation_pause(c.document, c.id.get()));
}
#[test]
fn callbacks_reenter_after_commit_and_a_later_control_wins() {
    let mut c = Context::new();
    let (a, _) = c.port();
    let (b, _) = c.port();
    let target = c.node(c.root);
    assert_eq!(divui_element_scroll_to(b, 0.0, 120.0), DivUiStatus::Ok);
    let mut r = range();
    r.present = 1;
    r.start = offset(6, 1, 50.0, 0.0);
    let id = c.animate(target, &source(a, 1), &r);
    let callback = Reentry {
        document: c.document,
        id: Cell::new(id),
        calls: Cell::new(0),
        set_status: Cell::new(DivUiStatus::Internal),
        pause_status: Cell::new(DivUiStatus::Internal),
    };
    let mut listener = ptr::null_mut();
    assert_eq!(
        divui_element_add_event_listener(
            target,
            20,
            0,
            Some(rebind_from_start),
            (&callback as *const Reentry).cast_mut().cast(),
            &mut listener
        ),
        DivUiStatus::Ok
    );
    assert_eq!(
        divui_document_set_animation_timeline_source_v1(c.document, id, &source(b, 1)),
        DivUiStatus::Ok
    );
    assert_eq!(callback.calls.get(), 1);
    assert_eq!(callback.set_status.get(), DivUiStatus::Ok);
    assert_eq!(callback.pause_status.get(), DivUiStatus::Ok);
    let state = animation(&c, id);
    // The range start is half the cover duration. Preserve CalculateCurrentTime
    // subtraction before the setTimeline EffectEnd ratio and ms reflection.
    let duration = (160.0 * 16.0 / 1000.0) / 1000.0;
    let current = (1920.0 / 1_000_000.0) - 0.5 * duration;
    assert_eq!(state.current_time_ms, (current / (duration * 0.5)) * 1000.0);
    assert_eq!(state.play_state, 1);
    let mut snapshot: DivUiAnimationTimelineSnapshotV1 = output();
    assert_eq!(
        divui_document_get_animation_timeline_snapshot_v1(c.document, id, &mut snapshot),
        DivUiStatus::Ok
    );
    assert_eq!(snapshot.present, 0);
    assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
}
