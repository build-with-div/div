//! Storage and mutation guards for caller-owned versioned inputs.

use super::*;

fn config() -> DivUiDocumentConfig {
    DivUiDocumentConfig {
        struct_size: size_of::<DivUiDocumentConfig>() as u32,
        abi_version: DIVUI_ABI_VERSION,
        viewport: DivUiViewportMetrics {
            logical_width: 16.0,
            logical_height: 16.0,
            physical_width: 16,
            physical_height: 16,
            device_scale_factor: 1.0,
            authority: 1,
            reserved: 0,
        },
    }
}

fn easing() -> DivUiEasing {
    DivUiEasing {
        kind: 0,
        step_position: 0,
        step_count: 0,
        reserved: 0,
        values: [0.0; 4],
    }
}

fn options() -> DivUiAnimationOptions {
    DivUiAnimationOptions {
        struct_size: size_of::<DivUiAnimationOptions>() as u32,
        abi_version: DIVUI_ABI_VERSION,
        delay_ms: 0.0,
        duration_ms: 100.0,
        iterations: 1.0,
        playback_rate: 1.0,
        direction: 0,
        fill: 3,
        play_state: 0,
        composite: 0,
        easing: easing(),
    }
}

struct Context {
    document: *mut DivUiDocument,
    root: *mut DivUiElement,
}

impl Context {
    fn new() -> Self {
        let mut document = ptr::null_mut();
        assert_eq!(
            divui_document_create(&config(), &mut document),
            DivUiStatus::Ok
        );
        let mut root = ptr::null_mut();
        assert_eq!(divui_document_root(document, &mut root), DivUiStatus::Ok);
        Self { document, root }
    }

    fn animate(
        &self,
        options: *const DivUiAnimationOptions,
        timeline: *const DivUiAnimationTimeline,
    ) -> DivUiStatus {
        let frames = [0.0f32, 1.0].map(|value| DivUiKeyframe {
            offset: f64::from(value),
            value: DivUiStyleValue {
                tag: 2,
                reserved: 0,
                data: DivUiStylePayload { number: value },
            },
            easing: easing(),
            has_easing: 0,
            reserved: 0,
        });
        let mut output = u64::MAX;
        let status = divui_element_animate(
            self.root,
            StyleProperty::Opacity as i32,
            frames.as_ptr(),
            frames.len(),
            options,
            timeline,
            &mut output,
        );
        // These calls are deliberately rejected before creating an animation.
        assert_eq!(output, u64::MAX);
        status
    }

    fn finish(self) {
        assert_eq!(divui_element_destroy(self.root), DivUiStatus::Ok);
        assert_eq!(divui_document_destroy(self.document), DivUiStatus::Ok);
    }
}

type Query = fn(&Context, *const c_void) -> DivUiStatus;

fn app(_: &Context, input: *const c_void) -> DivUiStatus {
    let mut output = ptr::dangling_mut::<DivUiApp>();
    let before = output;
    let status = divui_app_create(input.cast(), &mut output);
    assert_eq!(output, before);
    status
}

fn document(_: &Context, input: *const c_void) -> DivUiStatus {
    let mut output = ptr::dangling_mut::<DivUiDocument>();
    let before = output;
    let status = divui_document_create(input.cast(), &mut output);
    assert_eq!(output, before);
    status
}

fn viewport(context: &Context, input: *const c_void) -> DivUiStatus {
    divui_document_set_viewport(context.document, input.cast())
}

fn font(context: &Context, input: *const c_void) -> DivUiStatus {
    let data = [0u8];
    let mut output = ptr::dangling_mut::<DivUiFontFace>();
    let before = output;
    let status = divui_document_register_font(
        context.document,
        data.as_ptr(),
        data.len(),
        input.cast(),
        &mut output,
    );
    assert_eq!(output, before);
    status
}

fn animation_options(context: &Context, input: *const c_void) -> DivUiStatus {
    context.animate(input.cast(), ptr::null())
}

fn animation_timeline(context: &Context, input: *const c_void) -> DivUiStatus {
    context.animate(&options(), input.cast())
}

const QUERIES: [Query; 6] = [
    app,
    document,
    viewport,
    font,
    animation_options,
    animation_timeline,
];

#[test]
fn short_versioned_inputs_are_rejected_without_reading_payload_or_mutating_document() {
    let context = Context::new();
    for query in QUERIES {
        let size_only = Box::new(4u32);
        assert_eq!(
            query(&context, (&*size_only as *const u32).cast()),
            DivUiStatus::InvalidArgument
        );
        assert_eq!(*size_only, 4);
        for (version, expected) in [
            (DIVUI_ABI_VERSION, DivUiStatus::InvalidArgument),
            (DIVUI_ABI_VERSION ^ 1, DivUiStatus::AbiMismatch),
        ] {
            let header = Box::new([8u32, version]);
            assert_eq!(query(&context, header.as_ptr().cast()), expected);
            assert_eq!(*header, [8, version]);
        }
    }
    let mut animating = 1;
    assert_eq!(
        divui_document_is_animating(context.document, &mut animating),
        DivUiStatus::Ok
    );
    assert_eq!(animating, 0);
    let mut actual = std::mem::MaybeUninit::<DivUiViewportMetrics>::uninit();
    assert_eq!(
        divui_document_get_viewport(context.document, actual.as_mut_ptr()),
        DivUiStatus::Ok
    );
    let actual = unsafe { actual.assume_init() };
    assert_eq!(
        (
            actual.logical_width,
            actual.logical_height,
            actual.physical_width,
            actual.physical_height
        ),
        (16.0, 16.0, 16, 16)
    );
    context.finish();
}

#[repr(C)]
struct Extended<T> {
    base: T,
    tail: [u8; 16],
}

#[test]
fn valid_inputs_accept_future_extents_and_preserve_caller_storage() {
    let mut document_config = Extended {
        base: config(),
        tail: [0xa5; 16],
    };
    document_config.base.struct_size = size_of::<Extended<DivUiDocumentConfig>>() as u32;
    let mut document = ptr::null_mut();
    assert_eq!(
        divui_document_create(&document_config.base, &mut document),
        DivUiStatus::Ok
    );
    assert_eq!(document_config.tail, [0xa5; 16]);
    assert_eq!(
        document_config.base.struct_size as usize,
        size_of::<Extended<DivUiDocumentConfig>>()
    );
    document_config.base.viewport.logical_width = 24.0;
    document_config.base.viewport.physical_width = 24;
    assert_eq!(
        divui_document_set_viewport(document, &document_config.base),
        DivUiStatus::Ok
    );
    assert_eq!(document_config.tail, [0xa5; 16]);
    let mut actual = std::mem::MaybeUninit::<DivUiViewportMetrics>::uninit();
    assert_eq!(
        divui_document_get_viewport(document, actual.as_mut_ptr()),
        DivUiStatus::Ok
    );
    assert_eq!(unsafe { actual.assume_init() }.logical_width, 24.0);

    let mut animation_options = Extended {
        base: options(),
        tail: [0xa5; 16],
    };
    animation_options.base.struct_size = size_of::<Extended<DivUiAnimationOptions>>() as u32;
    assert!(super::animation_options(&animation_options.base).is_ok());
    assert_eq!(animation_options.tail, [0xa5; 16]);
    let mut timeline = Extended {
        base: DivUiAnimationTimeline {
            struct_size: size_of::<Extended<DivUiAnimationTimeline>>() as u32,
            abi_version: DIVUI_ABI_VERSION,
            kind: 0,
            axis: 0,
            source: ptr::null_mut(),
            range_start: 0.0,
            range_end: 1.0,
        },
        tail: [0xa5; 16],
    };
    let state = super::document(document as usize).unwrap();
    assert!(matches!(
        super::animation_timeline(&state, &timeline.base),
        Ok(AnimationTimeline::Document)
    ));
    assert!(matches!(
        super::animation_timeline(&state, ptr::null()),
        Ok(AnimationTimeline::Document)
    ));
    assert_eq!(timeline.tail, [0xa5; 16]);
    // An ABI mismatch in a future-sized input leaves its values untouched.
    timeline.base.abi_version ^= 1;
    assert!(super::animation_timeline(&state, &timeline.base).is_err());
    assert_eq!(timeline.tail, [0xa5; 16]);
    assert_eq!(divui_document_destroy(document), DivUiStatus::Ok);
}
