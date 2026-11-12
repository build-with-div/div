#![no_main]

use libfuzzer_sys::fuzz_target;
use div_ffi::*;
use std::mem::size_of;
use std::ptr;

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let config = DivUiDocumentConfig {
        struct_size: size_of::<DivUiDocumentConfig>() as u32,
        abi_version: DIVUI_ABI_VERSION,
        viewport: DivUiViewportMetrics {
            logical_width: 64.0,
            logical_height: 64.0,
            physical_width: 64,
            physical_height: 64,
            device_scale_factor: 1.0,
            authority: 1,
            reserved: 0,
        },
    };
    let mut document = ptr::null_mut();
    if divui_document_create(&config, &mut document) != DivUiStatus::Ok {
        return;
    }
    let mut element = ptr::null_mut();
    if divui_element_create(document, i32::from(data[0] % 39), &mut element) == DivUiStatus::Ok {
        // Tags 1..=5 contain no pointers. A compound property paired with any
        // of these tags is rejected before its union payload is interpreted.
        let value = DivUiStyleValue {
            tag: u32::from(data[1] % 6),
            reserved: u32::from(data[2] & 1),
            data: DivUiStylePayload {
                integer: i32::from_le_bytes([
                    data[3],
                    *data.get(4).unwrap_or(&0),
                    *data.get(5).unwrap_or(&0),
                    *data.get(6).unwrap_or(&0),
                ]),
            },
        };
        let property = i32::from(*data.get(7).unwrap_or(&0));
        let _ = divui_element_set_property(element, property, &value);
        let _ = divui_element_destroy(element);
    }
    let _ = divui_document_destroy(document);
});
