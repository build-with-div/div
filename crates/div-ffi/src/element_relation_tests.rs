use super::*;

#[test]
fn relation_queries_reject_engine_and_handle_map_borrows_without_transferring_ownership() {
    let config = DivUiDocumentConfig {
        struct_size: size_of::<DivUiDocumentConfig>() as u32,
        abi_version: DIVUI_ABI_VERSION,
        viewport: DivUiViewportMetrics {
            logical_width: 320.,
            logical_height: 200.,
            physical_width: 320,
            physical_height: 200,
            device_scale_factor: 1.,
            authority: 1,
            reserved: 0,
        },
    };
    let mut doc = ptr::null_mut();
    let mut body = ptr::null_mut();
    let mut form = ptr::null_mut();
    let mut input = ptr::null_mut();
    assert_eq!(divui_document_create(&config, &mut doc), DivUiStatus::Ok);
    assert_eq!(divui_document_root(doc, &mut body), DivUiStatus::Ok);
    assert_eq!(divui_element_create(doc, 35, &mut form), DivUiStatus::Ok);
    assert_eq!(divui_element_create(doc, 23, &mut input), DivUiStatus::Ok);
    assert_eq!(divui_element_append_child(body, form), DivUiStatus::Ok);
    assert_eq!(divui_element_append_child(form, input), DivUiStatus::Ok);
    let state = document(doc as usize).unwrap();
    for query in [divui_element_associated_form_v1, divui_element_parent_v1] {
        let before: usize = state.element_handles.borrow().values().map(Vec::len).sum();
        let mut out = 1usize as *mut DivUiElement;
        let engine = borrow_engine_mut(&state).unwrap();
        assert_eq!(query(input, &mut out), DivUiStatus::Reentrant);
        assert_eq!(out, 1usize as *mut DivUiElement);
        drop(engine);
        let map = state.element_handles.borrow_mut();
        assert_eq!(query(input, &mut out), DivUiStatus::Reentrant);
        assert_eq!(out, 1usize as *mut DivUiElement);
        drop(map);
        assert_eq!(
            state
                .element_handles
                .borrow()
                .values()
                .map(Vec::len)
                .sum::<usize>(),
            before
        );
        assert_eq!(query(input, &mut out), DivUiStatus::Ok);
        assert!(!out.is_null());
        assert_eq!(
            state
                .element_handles
                .borrow()
                .values()
                .map(Vec::len)
                .sum::<usize>(),
            before + 1
        );
        assert_eq!(divui_element_destroy(out), DivUiStatus::Ok);
        assert_eq!(
            state
                .element_handles
                .borrow()
                .values()
                .map(Vec::len)
                .sum::<usize>(),
            before
        );
    }
    assert_eq!(divui_document_destroy(doc), DivUiStatus::Ok);
}
