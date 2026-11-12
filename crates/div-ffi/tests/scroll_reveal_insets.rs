//! C ABI consumers share the native Engine reveal destination and ownership.
use div_ffi::*;
use div_style::{Overflow, StyleProperty};
use std::{mem::size_of, ptr};

struct Fixture {
    document: *mut DivUiDocument,
    elements: Vec<*mut DivUiElement>,
    port: *mut DivUiElement,
    target: *mut DivUiElement,
}

fn set_length(node: *mut DivUiElement, property: StyleProperty, value: f32) {
    assert_eq!(
        divui_element_set_property(
            node,
            property as i32,
            &DivUiStyleValue {
                tag: 1,
                reserved: 0,
                data: DivUiStylePayload {
                    length: DivUiLength { value, unit: 0 }
                },
            }
        ),
        DivUiStatus::Ok
    );
}

fn set_enum(node: *mut DivUiElement, property: StyleProperty, value: i32) {
    assert_eq!(
        divui_element_set_property(
            node,
            property as i32,
            &DivUiStyleValue {
                tag: 5,
                reserved: 0,
                data: DivUiStylePayload { enum_value: value },
            }
        ),
        DivUiStatus::Ok
    );
}

fn offset(node: *mut DivUiElement) -> (f64, f64) {
    let (mut x, mut y) = (f64::NAN, f64::NAN);
    assert_eq!(
        divui_element_get_scroll_offset(node, &mut x, &mut y),
        DivUiStatus::Ok
    );
    (x, y)
}

impl Fixture {
    fn new() -> Self {
        let mut document = ptr::null_mut();
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
                        reserved: 0,
                    },
                },
                &mut document
            ),
            DivUiStatus::Ok
        );
        let mut root = ptr::null_mut();
        assert_eq!(divui_document_root(document, &mut root), DivUiStatus::Ok);
        let mut elements = vec![root];
        let mut create = |parent| {
            let mut node = ptr::null_mut();
            // DIVUI_TAG_DIV is the existing C tag0, with native block defaults.
            assert_eq!(
                divui_element_create(document, 0, &mut node),
                DivUiStatus::Ok
            );
            assert_eq!(divui_element_append_child(parent, node), DivUiStatus::Ok);
            elements.push(node);
            node
        };
        let port = create(root);
        let target = create(port);
        let tail = create(port);
        for (node, property, value) in [
            (port, StyleProperty::Width, 100.0),
            (port, StyleProperty::Height, 80.0),
            (target, StyleProperty::Width, 20.0),
            (target, StyleProperty::Height, 20.0),
            (target, StyleProperty::MarginLeft, 150.0),
            (target, StyleProperty::MarginTop, 120.0),
            (tail, StyleProperty::Width, 400.0),
            (tail, StyleProperty::Height, 300.0),
            (port, StyleProperty::ScrollPaddingLeft, 20.0),
            (port, StyleProperty::ScrollPaddingTop, 25.0),
            (port, StyleProperty::ScrollPaddingRight, 5.0),
            (port, StyleProperty::ScrollPaddingBottom, 10.0),
            (target, StyleProperty::ScrollMarginLeft, 10.0),
            (target, StyleProperty::ScrollMarginTop, 15.0),
            (target, StyleProperty::ScrollMarginRight, 9.0),
            (target, StyleProperty::ScrollMarginBottom, 7.0),
        ] {
            set_length(node, property, value);
        }
        set_enum(port, StyleProperty::Overflow, Overflow::Hidden as i32);
        let mut width = DivUiStyleValue {
            tag: 0,
            reserved: 0,
            data: DivUiStylePayload { integer: 0 },
        };
        assert_eq!(
            divui_style_value_parse(
                StyleProperty::ScrollbarWidth as i32,
                DivUiUtf8 {
                    data: b"none".as_ptr(),
                    length: 4
                },
                &mut width
            ),
            DivUiStatus::Ok
        );
        assert_eq!(
            divui_element_set_property(port, StyleProperty::ScrollbarWidth as i32, &width),
            DivUiStatus::Ok
        );
        assert_eq!(
            divui_style_compound_destroy(unsafe { width.data.compound } as *mut DivUiStyleCompound),
            DivUiStatus::Ok
        );
        Self {
            document,
            elements,
            port,
            target,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for node in self.elements.drain(..).rev() {
            assert_eq!(divui_element_destroy(node), DivUiStatus::Ok);
        }
        assert_eq!(divui_document_destroy(self.document), DivUiStatus::Ok);
    }
}

#[test]
fn c_instant_smooth_and_accessibility_reveal_use_shared_native_insets() {
    let fixture = Fixture::new();
    assert_eq!(
        divui_element_scroll_into_view_v1(fixture.target, 0, 0, 1),
        DivUiStatus::Ok
    );
    assert_eq!(offset(fixture.port), (120.0, 80.0));
    assert_eq!(
        divui_element_scroll_to(fixture.port, 0.0, 0.0),
        DivUiStatus::Ok
    );
    assert_eq!(
        divui_element_smooth_scroll_into_view_v1(fixture.target, 0, 0, 1, 100.0),
        DivUiStatus::Ok
    );
    assert_eq!(offset(fixture.port), (0.0, 0.0));
    assert_eq!(
        divui_document_set_animation_time(fixture.document, 100.0),
        DivUiStatus::Ok
    );
    assert_eq!(offset(fixture.port), (120.0, 80.0));
    assert_eq!(
        divui_element_scroll_to(fixture.port, 0.0, 0.0),
        DivUiStatus::Ok
    );
    assert_eq!(
        divui_element_perform_accessibility_action(
            fixture.target,
            10,
            DivUiUtf8 {
                data: ptr::null(),
                length: 0
            },
            0,
            0
        ),
        DivUiStatus::Ok
    );
    assert_eq!(offset(fixture.port), (84.0, 77.0));
}

#[test]
fn c_reveal_validation_and_thread_ownership_preserve_current_destination() {
    let fixture = Fixture::new();
    assert_eq!(
        divui_element_scroll_into_view_v1(fixture.target, 0, 0, 1),
        DivUiStatus::Ok
    );
    assert_eq!(
        divui_element_scroll_into_view_v1(fixture.target, 4, 0, 1),
        DivUiStatus::InvalidArgument
    );
    assert_eq!(
        divui_element_smooth_scroll_into_view_v1(fixture.target, 0, 0, 1, f64::NAN),
        DivUiStatus::InvalidArgument
    );
    let address = fixture.target as usize;
    assert_eq!(
        std::thread::spawn(move || divui_element_scroll_into_view_v1(
            address as *mut DivUiElement,
            0,
            0,
            1
        ))
        .join()
        .unwrap(),
        DivUiStatus::WrongThread
    );
    assert_eq!(offset(fixture.port), (120.0, 80.0));
}
