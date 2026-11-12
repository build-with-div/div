//! Real property-bound carriers feed the same native Engine axis/source masks.
use super::*;
use std::mem::{align_of, MaybeUninit};

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
    let (mut doc, mut root, mut node) = (ptr::null_mut(), ptr::null_mut(), ptr::null_mut());
    assert_eq!(divui_document_create(&config, &mut doc), DivUiStatus::Ok);
    assert_eq!(divui_document_root(doc, &mut root), DivUiStatus::Ok);
    assert_eq!(divui_element_create(doc, 0, &mut node), DivUiStatus::Ok);
    assert_eq!(divui_element_append_child(root, node), DivUiStatus::Ok);
    (doc, root, node)
}
fn finish(doc: *mut DivUiDocument, root: *mut DivUiElement, node: *mut DivUiElement) {
    assert_eq!(divui_element_destroy(node), DivUiStatus::Ok);
    assert_eq!(divui_element_destroy(root), DivUiStatus::Ok);
    assert_eq!(divui_document_destroy(doc), DivUiStatus::Ok);
}
fn input(value: f32, offset: f32, unit: u32) -> DivUiRadiusAxisLengthV1 {
    DivUiRadiusAxisLengthV1 {
        struct_size: size_of::<DivUiRadiusAxisLengthV1>() as u32,
        abi_version: DIVUI_ABI_VERSION,
        value,
        calc_offset: offset,
        unit,
        reserved: 0,
    }
}
fn carrier(property: StyleProperty, input: &DivUiRadiusAxisLengthV1) -> DivUiStyleValue {
    let mut output = MaybeUninit::<DivUiStyleValue>::uninit();
    assert_eq!(
        divui_style_value_radius_axis_length_v1(property as i32, input, output.as_mut_ptr()),
        DivUiStatus::Ok
    );
    // SAFETY: successful factory initialized the complete tagged value.
    unsafe { output.assume_init() }
}
fn get(node: *mut DivUiElement, property: StyleProperty) -> DivUiRadiusAxisLengthV1 {
    let mut output = input(-91.0, -92.0, 99);
    assert_eq!(
        divui_element_radius_axis_length_v1(node, property as i32, &mut output),
        DivUiStatus::Ok
    );
    output
}
fn set(node: *mut DivUiElement, property: StyleProperty, value: DivUiRadiusAxisLengthV1) {
    let value = carrier(property, &value);
    assert_eq!(
        divui_element_set_property(node, property as i32, &value),
        DivUiStatus::Ok
    );
    // SAFETY: the successful property-bound factory returns a tag6 compound.
    assert_eq!(
        divui_style_compound_destroy(unsafe { value.data.compound }.cast_mut()),
        DivUiStatus::Ok
    );
}
fn sentinel() -> DivUiStyleValue {
    DivUiStyleValue {
        tag: 0x11111111,
        reserved: 0x22222222,
        data: DivUiStylePayload {
            compound: 0x3333333333333333_usize as *const DivUiStyleCompound,
        },
    }
}
fn bytes<T>(value: &T) -> Vec<u8> {
    // Callers here use fully initialized repr(C) records without padding.
    unsafe { std::slice::from_raw_parts((value as *const T).cast(), size_of::<T>()) }.to_vec()
}

#[test]
fn owned_axis_carriers_preserve_independent_sources_masks_and_removal() {
    assert_eq!(
        (
            size_of::<DivUiRadiusAxisLengthV1>(),
            align_of::<DivUiRadiusAxisLengthV1>()
        ),
        (24, 4)
    );
    let (doc, root, node) = fixture();
    for (index, property) in div_style::RADIUS_AXIS_PROPERTIES.into_iter().enumerate() {
        set(node, property, input(index as f32 + 1.0, 0.0, 0));
        let value = get(node, property);
        assert_eq!(
            (value.value, value.calc_offset, value.unit, value.reserved),
            (index as f32 + 1.0, 0.0, 0, 0)
        );
    }
    let x = StyleProperty::BorderTopLeftRadiusX;
    let y = StyleProperty::BorderTopLeftRadiusY;
    set(node, x, input(25.0, -3.0, 13));
    set(node, y, input(1.5, 0.0, 2));
    let xv = get(node, x);
    let yv = get(node, y);
    assert_eq!((xv.value, xv.calc_offset, xv.unit), (25.0, -3.0, 13));
    assert_eq!((yv.value, yv.calc_offset, yv.unit), (24.0, 0.0, 0));
    let state = document(doc as usize).unwrap();
    let handle = element(node as usize).unwrap().node;
    assert_eq!(
        borrow_engine(&state)
            .unwrap()
            .radius_axis_length(handle, x)
            .unwrap(),
        LengthValue::calc_percent_px(25.0, -3.0)
    );
    let mut removed = 77;
    assert_eq!(
        divui_element_remove_property(node, x as i32, &mut removed),
        DivUiStatus::Ok
    );
    assert_eq!(removed, 1);
    assert_eq!(get(node, x).value, 0.0);
    assert_eq!(get(node, y).value, 24.0);
    let before = borrow_engine(&state).unwrap().dirty_generations();
    assert_eq!(
        divui_element_remove_property(node, x as i32, &mut removed),
        DivUiStatus::Ok
    );
    assert_eq!(removed, 0);
    assert_eq!(borrow_engine(&state).unwrap().dirty_generations(), before);
    drop(state);
    finish(doc, root, node);
}

#[test]
fn axis_prefix_errors_are_transactional_and_future_tails_are_owned() {
    let property = StyleProperty::BorderTopLeftRadiusX;
    let mut output = sentinel();
    let original = bytes(&output);
    let size_only = 4_u32;
    assert_eq!(
        divui_style_value_radius_axis_length_v1(
            property as i32,
            (&size_only as *const u32).cast(),
            &mut output
        ),
        DivUiStatus::InvalidArgument
    );
    let short = [8_u32, DIVUI_ABI_VERSION];
    assert_eq!(
        divui_style_value_radius_axis_length_v1(
            property as i32,
            short.as_ptr().cast(),
            &mut output
        ),
        DivUiStatus::InvalidArgument
    );
    let bad_abi = [24_u32, DIVUI_ABI_VERSION + 1];
    assert_eq!(
        divui_style_value_radius_axis_length_v1(
            property as i32,
            bad_abi.as_ptr().cast(),
            &mut output
        ),
        DivUiStatus::AbiMismatch
    );
    assert_eq!(bytes(&output), original);
    for value in [
        input(-1.0, 0.0, 0),
        input(f32::NAN, 0.0, 1),
        input(1.0, f32::INFINITY, 13),
        input(0.0, 0.0, 6),
        input(0.0, 0.0, 7),
        input(1.0, 2.0, 0),
        input(1.0, 0.0, 14),
    ] {
        assert_eq!(
            divui_style_value_radius_axis_length_v1(property as i32, &value, &mut output),
            DivUiStatus::InvalidArgument
        );
        assert_eq!(bytes(&output), original);
    }
    let mut reserved = input(1.0, 0.0, 0);
    reserved.reserved = 1;
    assert_eq!(
        divui_style_value_radius_axis_length_v1(property as i32, &reserved, &mut output),
        DivUiStatus::InvalidArgument
    );
    assert_eq!(bytes(&output), original);
    #[repr(C)]
    struct Future {
        prefix: DivUiRadiusAxisLengthV1,
        tail: [u8; 8],
    }
    let mut future = Future {
        prefix: input(30.0, 2.0, 13),
        tail: [0xa5; 8],
    };
    future.prefix.struct_size = size_of::<Future>() as u32;
    let value = carrier(property, &future.prefix);
    future.prefix.value = 99.0;
    let (doc, root, node) = fixture();
    assert_eq!(
        divui_element_set_property(node, property as i32, &value),
        DivUiStatus::Ok
    );
    assert_eq!(
        divui_element_set_property(node, StyleProperty::BorderTopLeftRadiusY as i32, &value),
        DivUiStatus::WrongValueType
    );
    assert_eq!(get(node, property).value, 30.0);
    // SAFETY: owned live property-bound compound, released on its owner thread.
    assert_eq!(
        divui_style_compound_destroy(unsafe { value.data.compound }.cast_mut()),
        DivUiStatus::Ok
    );
    let mut future_out = Future {
        prefix: input(-1.0, -2.0, 99),
        tail: [0x5a; 8],
    };
    future_out.prefix.struct_size = size_of::<Future>() as u32;
    assert_eq!(
        divui_element_radius_axis_length_v1(node, property as i32, &mut future_out.prefix),
        DivUiStatus::Ok
    );
    assert_eq!(
        (
            future_out.prefix.value,
            future_out.prefix.calc_offset,
            future_out.prefix.unit
        ),
        (30.0, 2.0, 13)
    );
    assert_eq!(future_out.prefix.struct_size, 32);
    assert_eq!(future_out.tail, [0x5a; 8]);
    finish(doc, root, node);
}

#[test]
fn axis_getter_thread_stale_handle_and_borrow_errors_leave_output_unchanged() {
    let (doc, root, node) = fixture();
    let property = StyleProperty::BorderBottomRightRadiusY;
    set(node, property, input(35.0, 0.0, 1));
    let state = document(doc as usize).unwrap();
    let mut out = input(-1.0, -2.0, 99);
    let original = bytes(&out);
    let held = borrow_engine_mut(&state).unwrap();
    assert_eq!(
        divui_element_radius_axis_length_v1(node, property as i32, &mut out),
        DivUiStatus::Reentrant
    );
    assert_eq!(bytes(&out), original);
    drop(held);
    let address = node as usize;
    let result = std::thread::spawn(move || {
        let mut out = input(-1.0, -2.0, 99);
        let before = bytes(&out);
        let status = divui_element_radius_axis_length_v1(
            address as *mut DivUiElement,
            property as i32,
            &mut out,
        );
        (status, bytes(&out) == before)
    })
    .join()
    .unwrap();
    assert_eq!(result, (DivUiStatus::WrongThread, true));
    let mut short = [8_u32, DIVUI_ABI_VERSION];
    let before = short;
    assert_eq!(
        divui_element_radius_axis_length_v1(node, property as i32, short.as_mut_ptr().cast()),
        DivUiStatus::InvalidArgument
    );
    assert_eq!(short, before);
    assert_eq!(
        divui_element_radius_axis_length_v1(node, StyleProperty::Width as i32, &mut out),
        DivUiStatus::WrongValueType
    );
    assert_eq!(bytes(&out), original);
    drop(state);
    assert_eq!(divui_element_destroy(node), DivUiStatus::Ok);
    assert_eq!(
        divui_element_radius_axis_length_v1(node, property as i32, &mut out),
        DivUiStatus::InvalidHandle
    );
    assert_eq!(bytes(&out), original);
    assert_eq!(divui_element_destroy(root), DivUiStatus::Ok);
    assert_eq!(divui_document_destroy(doc), DivUiStatus::Ok);
}
