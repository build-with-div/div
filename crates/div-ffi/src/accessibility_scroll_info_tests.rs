//! Public owned C scroll-info storage/lifecycle guards; no alternate engine.
use super::accessibility_snapshot::{
    divui_accessibility_snapshot_destroy, divui_document_accessibility_snapshot,
};
use super::*;
use std::mem::align_of;

fn output() -> DivUiAccessibilityScrollInfoV1 {
    DivUiAccessibilityScrollInfoV1 {
        struct_size: size_of::<DivUiAccessibilityScrollInfoV1>() as u32,
        abi_version: DIVUI_ABI_VERSION,
        ..Default::default()
    }
}
fn sentinel() -> DivUiAccessibilityScrollInfoV1 {
    DivUiAccessibilityScrollInfoV1 {
        flags: 0xa5a5_a5a5,
        actions: 0x5a5a_5a5a,
        scroll_x: 1.25,
        scroll_y: -2.5,
        scroll_x_min: -3.75,
        scroll_x_max: 4.25,
        scroll_y_min: -5.5,
        scroll_y_max: 6.75,
        ..output()
    }
}
fn property(element: *mut DivUiElement, prop: StyleProperty, text: &str) {
    let mut value: DivUiStyleValue = unsafe { std::mem::zeroed() };
    assert_eq!(
        divui_style_value_parse(
            prop as i32,
            DivUiUtf8 {
                data: text.as_ptr(),
                length: text.len(),
            },
            &mut value
        ),
        DivUiStatus::Ok
    );
    assert_eq!(
        divui_element_set_property(element, prop as i32, &value),
        DivUiStatus::Ok
    );
    if value.tag == 6 {
        assert_eq!(
            divui_style_compound_destroy(unsafe { value.data.compound }.cast_mut()),
            DivUiStatus::Ok
        );
    }
}
struct Fixture {
    document: *mut DivUiDocument,
    root: *mut DivUiElement,
    scroll: *mut DivUiElement,
    child: *mut DivUiElement,
    id: u64,
}
impl Fixture {
    fn new(scale: f64) -> Self {
        let config = DivUiDocumentConfig {
            struct_size: size_of::<DivUiDocumentConfig>() as u32,
            abi_version: DIVUI_ABI_VERSION,
            viewport: DivUiViewportMetrics {
                logical_width: 320.0,
                logical_height: 240.0,
                physical_width: (320.0 * scale) as u32,
                physical_height: (240.0 * scale) as u32,
                device_scale_factor: scale,
                authority: 1,
                reserved: 0,
            },
        };
        let (mut document, mut root, mut scroll, mut child) = (
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        );
        assert_eq!(
            divui_document_create(&config, &mut document),
            DivUiStatus::Ok
        );
        assert_eq!(divui_document_root(document, &mut root), DivUiStatus::Ok);
        assert_eq!(
            divui_element_create(document, 0, &mut scroll),
            DivUiStatus::Ok
        );
        assert_eq!(
            divui_element_create(document, 0, &mut child),
            DivUiStatus::Ok
        );
        assert_eq!(divui_element_append_child(root, scroll), DivUiStatus::Ok);
        assert_eq!(divui_element_append_child(scroll, child), DivUiStatus::Ok);
        for (element, prop, literal) in [
            (scroll, StyleProperty::Width, "100px"),
            (scroll, StyleProperty::Height, "80px"),
            (scroll, StyleProperty::Overflow, "auto"),
            (scroll, StyleProperty::ScrollbarWidth, "none"),
            (child, StyleProperty::Width, "160px"),
            (child, StyleProperty::Height, "240px"),
        ] {
            property(element, prop, literal);
        }
        let mut id = 0;
        assert_eq!(
            divui_element_get_accessibility_id(scroll, &mut id),
            DivUiStatus::Ok
        );
        Self {
            document,
            root,
            scroll,
            child,
            id,
        }
    }
    fn destroy(self) {
        for element in [self.child, self.scroll, self.root] {
            assert_eq!(divui_element_destroy(element), DivUiStatus::Ok);
        }
        assert_eq!(divui_document_destroy(self.document), DivUiStatus::Ok);
    }
}
fn capture(
    document: *mut DivUiDocument,
    previous: *const DivUiAccessibilitySnapshot,
) -> *mut DivUiAccessibilitySnapshot {
    let mut result = ptr::null_mut();
    assert_eq!(
        divui_document_accessibility_snapshot(document, previous, &mut result),
        DivUiStatus::Ok
    );
    result
}
fn info(snapshot: *const DivUiAccessibilitySnapshot, id: u64) -> DivUiAccessibilityScrollInfoV1 {
    let mut result = output();
    assert_eq!(
        divui_accessibility_snapshot_get_scroll_info_v1(snapshot, id, &mut result),
        DivUiStatus::Ok
    );
    result
}
fn all_flags() -> u32 {
    DIVUI_ACCESSIBILITY_SCROLL_HAS_X
        | DIVUI_ACCESSIBILITY_SCROLL_HAS_Y
        | DIVUI_ACCESSIBILITY_SCROLL_HAS_X_MIN
        | DIVUI_ACCESSIBILITY_SCROLL_HAS_X_MAX
        | DIVUI_ACCESSIBILITY_SCROLL_HAS_Y_MIN
        | DIVUI_ACCESSIBILITY_SCROLL_HAS_Y_MAX
}

#[test]
fn c_owned_scroll_info_preserves_policy_ranges_and_values_after_document_destruction() {
    for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
        let f = Fixture::new(scale);
        let initial = capture(f.document, ptr::null());
        let first = info(initial, f.id);
        assert_eq!(first.flags, all_flags());
        assert_eq!(
            first.actions,
            DIVUI_ACCESSIBILITY_SCROLL_DIRECTION_ACTION_MASK
        );
        assert_eq!((first.scroll_x, first.scroll_y), (0.0, 0.0));
        assert_eq!((first.scroll_x_min, first.scroll_x_max), (0.0, 60.0));
        assert_eq!((first.scroll_y_min, first.scroll_y_max), (0.0, 160.0));
        assert_eq!(
            divui_element_scroll_to(f.scroll, 20.0, 40.0),
            DivUiStatus::Ok
        );
        property(f.scroll, StyleProperty::Overflow, "hidden");
        let hidden = capture(f.document, initial);
        let hidden_info = info(hidden, f.id);
        assert_eq!(hidden_info.flags, all_flags());
        assert_eq!(hidden_info.actions, 0);
        assert_eq!((hidden_info.scroll_x, hidden_info.scroll_y), (20.0, 40.0));
        assert_eq!(
            (hidden_info.scroll_x_max, hidden_info.scroll_y_max),
            (60.0, 160.0)
        );
        property(f.scroll, StyleProperty::Overflow, "clip");
        let clipped = capture(f.document, hidden);
        let clipped_info = info(clipped, f.id);
        assert_eq!(clipped_info, output());
        property(f.scroll, StyleProperty::Direction, "rtl");
        property(f.scroll, StyleProperty::Overflow, "auto");
        assert_eq!(
            divui_element_scroll_to(f.scroll, -20.0, 40.0),
            DivUiStatus::Ok
        );
        let rtl = capture(f.document, clipped);
        let rtl_info = info(rtl, f.id);
        assert_eq!(rtl_info.flags, all_flags());
        assert_eq!(
            rtl_info.actions,
            DIVUI_ACCESSIBILITY_SCROLL_DIRECTION_ACTION_MASK
        );
        assert_eq!((rtl_info.scroll_x, rtl_info.scroll_y), (-20.0, 40.0));
        assert_eq!((rtl_info.scroll_x_min, rtl_info.scroll_x_max), (-60.0, 0.0));
        let id = f.id;
        f.destroy();
        for (snapshot, expected) in [
            (initial, first),
            (hidden, hidden_info),
            (clipped, clipped_info),
            (rtl, rtl_info),
        ] {
            assert_eq!(info(snapshot, id), expected);
            assert_eq!(
                divui_accessibility_snapshot_destroy(snapshot),
                DivUiStatus::Ok
            );
        }
    }
}

#[test]
fn c_scroll_info_checks_initialized_prefix_before_full_output_and_preserves_failures() {
    assert_eq!(
        (
            size_of::<DivUiAccessibilityScrollInfoV1>(),
            align_of::<DivUiAccessibilityScrollInfoV1>()
        ),
        (64, 8)
    );
    assert_eq!(size_of::<DivUiAccessibilityNodeState>(), 112);
    assert_eq!(size_of::<DivUiAccessibilityNodeInfo>(), 120);
    assert_eq!(size_of::<DivUiAccessibilitySnapshotInfo>(), 56);
    let f = Fixture::new(1.0);
    let snapshot = capture(f.document, ptr::null());
    assert_eq!(
        divui_accessibility_snapshot_get_scroll_info_v1(snapshot, f.id, ptr::null_mut()),
        DivUiStatus::InvalidArgument
    );
    // These allocations contain only the initialized header prefix. Rejection
    // must not form a full T view, read an unavailable ABI word, or write data.
    let mut four = 4u32;
    assert_eq!(
        divui_accessibility_snapshot_get_scroll_info_v1(
            snapshot,
            f.id,
            (&mut four as *mut u32).cast()
        ),
        DivUiStatus::InvalidArgument
    );
    assert_eq!(four, 4);
    let mut eight = [8u32, DIVUI_ABI_VERSION];
    assert_eq!(
        divui_accessibility_snapshot_get_scroll_info_v1(snapshot, f.id, eight.as_mut_ptr().cast()),
        DivUiStatus::InvalidArgument
    );
    assert_eq!(eight, [8, DIVUI_ABI_VERSION]);
    for (size, abi, id, expected) in [
        (63, DIVUI_ABI_VERSION, f.id, DivUiStatus::InvalidArgument),
        (64, 0, f.id, DivUiStatus::AbiMismatch),
        (
            64,
            DIVUI_ABI_VERSION,
            u64::MAX,
            DivUiStatus::InvalidArgument,
        ),
    ] {
        let mut out = DivUiAccessibilityScrollInfoV1 {
            struct_size: size,
            abi_version: abi,
            ..sentinel()
        };
        let before = out;
        assert_eq!(
            divui_accessibility_snapshot_get_scroll_info_v1(snapshot, id, &mut out),
            expected
        );
        assert_eq!(out, before);
    }
    #[repr(C)]
    struct Future {
        info: DivUiAccessibilityScrollInfoV1,
        tail: [u64; 2],
    }
    let mut future = Future {
        info: DivUiAccessibilityScrollInfoV1 {
            struct_size: size_of::<Future>() as u32,
            ..output()
        },
        tail: [0x0123_4567_89ab_cdef, 0xfedc_ba98_7654_3210],
    };
    assert_eq!(
        divui_accessibility_snapshot_get_scroll_info_v1(snapshot, f.id, &mut future.info),
        DivUiStatus::Ok
    );
    assert_eq!(future.info.struct_size, size_of::<Future>() as u32);
    assert_eq!(future.tail, [0x0123_4567_89ab_cdef, 0xfedc_ba98_7654_3210]);
    assert_eq!(future.info.scroll_y_max, 160.0);
    assert_eq!(
        divui_accessibility_snapshot_destroy(snapshot),
        DivUiStatus::Ok
    );
    let mut out = sentinel();
    let before = out;
    assert_eq!(
        divui_accessibility_snapshot_get_scroll_info_v1(snapshot, f.id, &mut out),
        DivUiStatus::InvalidHandle
    );
    assert_eq!(out, before);
    f.destroy();
}

#[test]
fn c_owned_scroll_info_enforces_snapshot_thread_without_touching_output() {
    let f = Fixture::new(1.0);
    let snapshot = capture(f.document, ptr::null());
    let address = snapshot as usize;
    let id = f.id;
    std::thread::spawn(move || {
        let mut out = sentinel();
        let before = out;
        assert_eq!(
            divui_accessibility_snapshot_get_scroll_info_v1(
                address as *const DivUiAccessibilitySnapshot,
                id,
                &mut out
            ),
            DivUiStatus::WrongThread
        );
        assert_eq!(out, before);
    })
    .join()
    .unwrap();
    assert_eq!(info(snapshot, f.id).scroll_y_max, 160.0);
    f.destroy();
    assert_eq!(info(snapshot, id).scroll_y_max, 160.0);
    assert_eq!(
        divui_accessibility_snapshot_destroy(snapshot),
        DivUiStatus::Ok
    );
}
