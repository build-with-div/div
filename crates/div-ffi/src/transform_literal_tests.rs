//! Existing owned style carriers must transport native transform literals with
//! the same lifecycle, thread validation and output-preservation rules.
use super::*;

fn text(value: &str) -> DivUiUtf8 {
    DivUiUtf8 {
        data: value.as_ptr(),
        length: value.len(),
    }
}
fn sentinel() -> DivUiStyleValue {
    DivUiStyleValue {
        tag: 3,
        reserved: 0x1234,
        data: DivUiStylePayload { integer: 42 },
    }
}
fn parsed(property: StyleProperty, input: &str) -> DivUiStyleValue {
    let mut value = sentinel();
    assert_eq!(
        divui_style_value_parse(property as i32, text(input), &mut value),
        DivUiStatus::Ok
    );
    value
}
fn property(element: *mut DivUiElement, property: StyleProperty, input: &str) {
    let value = parsed(property, input);
    assert_eq!(
        divui_element_set_property(element, property as i32, &value),
        DivUiStatus::Ok
    );
    if value.tag == 6 {
        // SAFETY: the successful parse identifies the active compound member.
        assert_eq!(
            divui_style_compound_destroy(unsafe { value.data.compound } as *mut _),
            DivUiStatus::Ok
        );
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
    for (property_name, input) in [
        (StyleProperty::Display, "block"),
        (StyleProperty::Position, "absolute"),
        (StyleProperty::Left, "0px"),
        (StyleProperty::Top, "0px"),
        (StyleProperty::Width, "100px"),
        (StyleProperty::Height, "40px"),
    ] {
        property(node, property_name, input);
    }
    (doc, root, node)
}
fn bounds(node: *mut DivUiElement) -> DivUiRect {
    let mut value = DivUiRect {
        x: -1.0,
        y: -1.0,
        width: -1.0,
        height: -1.0,
    };
    assert_eq!(divui_element_get_bounds(node, &mut value), DivUiStatus::Ok);
    value
}
fn cleanup(doc: *mut DivUiDocument, root: *mut DivUiElement, node: *mut DivUiElement) {
    assert_eq!(divui_element_destroy(node), DivUiStatus::Ok);
    assert_eq!(divui_element_destroy(root), DivUiStatus::Ok);
    assert_eq!(divui_document_destroy(doc), DivUiStatus::Ok);
}

#[test]
fn parsed_transform_carrier_is_owned_and_resolves_mutated_reference_boxes() {
    let (doc, root, node) = fixture();
    let value = parsed(StyleProperty::Transform, "translate(50%,25%)");
    assert_eq!(value.tag, 6);
    let compound = unsafe { value.data.compound } as *mut DivUiStyleCompound;
    assert_eq!(
        divui_element_set_property(node, StyleProperty::Transform as i32, &value),
        DivUiStatus::Ok
    );
    assert_eq!(divui_style_compound_destroy(compound), DivUiStatus::Ok);
    let first = bounds(node);
    assert_eq!(
        (first.x, first.y, first.width, first.height),
        (50.0, 10.0, 100.0, 40.0)
    );
    property(node, StyleProperty::Width, "200px");
    property(node, StyleProperty::Height, "80px");
    let second = bounds(node);
    assert_eq!(
        (second.x, second.y, second.width, second.height),
        (100.0, 20.0, 200.0, 80.0)
    );
    assert_eq!(
        divui_element_set_property(node, StyleProperty::Transform as i32, &value),
        DivUiStatus::InvalidHandle
    );
    assert_eq!(bounds(node), second);
    property(node, StyleProperty::Transform, "translate(20px,7px)");
    assert_eq!((bounds(node).x, bounds(node).y), (20.0, 7.0));
    property(node, StyleProperty::Transform, "scale(2,3)");
    let scaled = bounds(node);
    assert_eq!((scaled.width, scaled.height), (400.0, 240.0));
    let mut client_width = -1;
    let mut client_height = -1;
    assert_eq!(
        divui_element_get_client_width_v1(node, &mut client_width),
        DivUiStatus::Ok
    );
    assert_eq!(
        divui_element_get_client_height_v1(node, &mut client_height),
        DivUiStatus::Ok
    );
    assert_eq!((client_width, client_height), (200, 80));
    property(node, StyleProperty::Transform, "none");
    assert_eq!((bounds(node).x, bounds(node).y), (0.0, 0.0));
    assert_eq!(divui_element_remove(node), DivUiStatus::Ok);
    let mut untouched = DivUiRect {
        x: -1.0,
        y: -2.0,
        width: -3.0,
        height: -4.0,
    };
    assert_eq!(
        divui_element_get_bounds(node, &mut untouched),
        DivUiStatus::StaleHandle
    );
    assert_eq!(
        (untouched.x, untouched.y, untouched.width, untouched.height),
        (-1.0, -2.0, -3.0, -4.0)
    );
    cleanup(doc, root, node);
}

#[test]
fn rejected_literal_and_wrong_property_preserve_output_and_retained_document() {
    let (doc, root, node) = fixture();
    property(node, StyleProperty::Transform, "translate(20px,7px)");
    let before = bounds(node);
    let state = document(doc as usize).unwrap();
    let before_stats = borrow_engine_mut(&state).unwrap().stats();
    for input in [
        "scale(NaN)",
        "scale(1e100)",
        "translate(20px,7px) junk",
        "translate(1px,2px); opacity:0",
    ] {
        let mut output = sentinel();
        assert_eq!(
            divui_style_value_parse(StyleProperty::Transform as i32, text(input), &mut output),
            DivUiStatus::InvalidArgument
        );
        assert_eq!((output.tag, output.reserved), (3, 0x1234));
        assert_eq!(unsafe { output.data.integer }, 42);
        assert_eq!(bounds(node), before);
    }
    assert_eq!(
        divui_style_value_parse(
            StyleProperty::Transform as i32,
            text("scale(2)"),
            ptr::null_mut()
        ),
        DivUiStatus::InvalidArgument
    );
    let transform = parsed(StyleProperty::Transform, "scale(2)");
    assert_eq!(
        divui_element_set_property(node, StyleProperty::Opacity as i32, &transform),
        DivUiStatus::WrongValueType
    );
    assert_eq!(bounds(node), before);
    let after_stats = borrow_engine_mut(&state).unwrap().stats();
    assert_eq!(after_stats.layouts, before_stats.layouts);
    assert_eq!(after_stats.paints, before_stats.paints);
    assert_eq!(
        divui_style_compound_destroy(unsafe { transform.data.compound } as *mut _),
        DivUiStatus::Ok
    );
    cleanup(doc, root, node);
}

#[test]
fn parsed_transform_carrier_rejects_foreign_thread_before_mutation_or_destruction() {
    let (doc, root, node) = fixture();
    let value = parsed(StyleProperty::Transform, "translate(20px,7px)");
    let compound = unsafe { value.data.compound } as usize;
    let node_address = node as usize;
    let status = std::thread::spawn(move || {
        let value = DivUiStyleValue {
            tag: 6,
            reserved: 0,
            data: DivUiStylePayload {
                compound: compound as *const DivUiStyleCompound,
            },
        };
        (
            divui_element_set_property(
                node_address as *mut DivUiElement,
                StyleProperty::Transform as i32,
                &value,
            ),
            divui_style_compound_destroy(compound as *mut DivUiStyleCompound),
        )
    })
    .join()
    .unwrap();
    assert_eq!(status, (DivUiStatus::WrongThread, DivUiStatus::WrongThread));
    assert_eq!((bounds(node).x, bounds(node).y), (0.0, 0.0));
    assert_eq!(
        divui_element_set_property(node, StyleProperty::Transform as i32, &value),
        DivUiStatus::Ok
    );
    assert_eq!((bounds(node).x, bounds(node).y), (20.0, 7.0));
    assert_eq!(
        divui_style_compound_destroy(compound as *mut DivUiStyleCompound),
        DivUiStatus::Ok
    );
    cleanup(doc, root, node);
}
