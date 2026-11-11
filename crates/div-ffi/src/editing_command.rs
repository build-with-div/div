//! Versioned native editing commands over the shared Rust document.

use super::*;
use div_engine::{EditCommand, TextDirection, TextUnit};

fn decode(raw: &DivUiEditCommandV1) -> Result<EditCommand, ApiError> {
    if raw.reserved != [0; 2] || raw.extend_selection > 1 {
        return Err(invalid(
            "editing command reserved or selection fields are invalid",
        ));
    }
    if raw.command != DIVUI_EDIT_MOVE && raw.extend_selection != 0 {
        return Err(invalid("only move commands can extend a selection"));
    }
    match raw.command {
        DIVUI_EDIT_MOVE | DIVUI_EDIT_DELETE => {
            let direction = match raw.direction {
                DIVUI_TEXT_BACKWARD => TextDirection::Backward,
                DIVUI_TEXT_FORWARD => TextDirection::Forward,
                _ => return Err(invalid("editing direction is invalid")),
            };
            let unit = match raw.unit {
                DIVUI_TEXT_GRAPHEME => TextUnit::Grapheme,
                DIVUI_TEXT_WORD => TextUnit::Word,
                DIVUI_TEXT_LINE => TextUnit::Line,
                DIVUI_TEXT_DOCUMENT => TextUnit::Document,
                _ => return Err(invalid("editing text unit is invalid")),
            };
            Ok(if raw.command == DIVUI_EDIT_MOVE {
                EditCommand::Move {
                    direction,
                    unit,
                    extend: raw.extend_selection != 0,
                }
            } else {
                EditCommand::Delete { direction, unit }
            })
        }
        DIVUI_EDIT_SELECT_ALL | DIVUI_EDIT_UNDO | DIVUI_EDIT_REDO => {
            if raw.direction != 0 || raw.unit != 0 {
                return Err(invalid("unused editing command fields must be zero"));
            }
            Ok(match raw.command {
                DIVUI_EDIT_SELECT_ALL => EditCommand::SelectAll,
                DIVUI_EDIT_UNDO => EditCommand::Undo,
                _ => EditCommand::Redo,
            })
        }
        _ => Err(invalid("editing command kind is invalid")),
    }
}

// SAFETY CONTRACT: `element` is owned by the calling thread. `command`
// points to aligned, readable storage for its declared struct_size bytes.
// It is copied, never retained. Listener user_data must remain live through
// synchronous dispatch. Callbacks run with no engine or facade borrow held.
#[no_mangle]
pub extern "C" fn divui_element_edit_text_v1(
    element_handle: *mut DivUiElement,
    command: *const DivUiEditCommandV1,
) -> DivUiStatus {
    ffi(|| {
        if command.is_null() {
            return Err(invalid("editing command is null"));
        }
        // SAFETY: even a short caller layout declares its first size field.
        // Do not read the ABI field or full payload until its size permits it.
        let struct_size = unsafe { ptr::read(command.cast::<u32>()) };
        if struct_size < 2 * size_of::<u32>() as u32 {
            return Err(invalid("editing command header is too small"));
        }
        // SAFETY: the declared header includes the ABI-version field.
        let abi_version = unsafe { ptr::read(command.cast::<u32>().add(1)) };
        check_header(struct_size, abi_version, size_of::<DivUiEditCommandV1>())?;
        // SAFETY: a valid header guarantees the current payload is readable.
        let raw = unsafe { ptr::read(command) };
        let command = decode(&raw)?;
        let element = element(element_handle as usize)?;
        let state = element_document(&element)?;
        state
            .native
            .edit_control_for_native_facade(element.node, command)
            .map_err(native_app::native_error)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> DivUiUtf8 {
        DivUiUtf8 {
            data: value.as_ptr(),
            length: value.len(),
        }
    }

    fn command(kind: u32) -> DivUiEditCommandV1 {
        DivUiEditCommandV1 {
            struct_size: size_of::<DivUiEditCommandV1>() as u32,
            abi_version: DIVUI_ABI_VERSION,
            command: kind,
            direction: 0,
            unit: 0,
            extend_selection: 0,
            reserved: [0; 2],
        }
    }

    struct Fixture {
        document: *mut DivUiDocument,
        root: *mut DivUiElement,
        input: *mut DivUiElement,
        sibling: *mut DivUiElement,
    }

    impl Fixture {
        fn new(tag: i32) -> Self {
            let config = DivUiDocumentConfig {
                struct_size: size_of::<DivUiDocumentConfig>() as u32,
                abi_version: DIVUI_ABI_VERSION,
                viewport: DivUiViewportMetrics {
                    logical_width: 320.0,
                    logical_height: 200.0,
                    physical_width: 320,
                    physical_height: 200,
                    device_scale_factor: 1.0,
                    authority: 1,
                    reserved: 0,
                },
            };
            let mut fixture = Self {
                document: ptr::null_mut(),
                root: ptr::null_mut(),
                input: ptr::null_mut(),
                sibling: ptr::null_mut(),
            };
            assert_eq!(
                divui_document_create(&config, &mut fixture.document),
                DivUiStatus::Ok
            );
            assert_eq!(
                divui_document_root(fixture.document, &mut fixture.root),
                DivUiStatus::Ok
            );
            for (tag, out) in [(tag, &mut fixture.input), (0, &mut fixture.sibling)] {
                assert_eq!(
                    divui_element_create(fixture.document, tag, out),
                    DivUiStatus::Ok
                );
                assert_eq!(
                    divui_element_append_child(fixture.root, *out),
                    DivUiStatus::Ok
                );
            }
            assert_eq!(
                divui_element_set_attribute(fixture.input, text("id"), text("editable")),
                DivUiStatus::Ok
            );
            fixture
        }

        fn native(&self) -> div::Element {
            document(self.document as usize)
                .unwrap()
                .native
                .element_by_id("editable")
                .unwrap()
                .unwrap()
        }

        fn set(&self, value: &str) {
            assert_eq!(
                divui_element_set_control_value(self.input, text(value)),
                DivUiStatus::Ok
            );
            assert_eq!(
                divui_element_set_selection(self.input, value.len(), value.len()),
                DivUiStatus::Ok
            );
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            for element in [self.input, self.sibling, self.root] {
                assert_eq!(divui_element_destroy(element), DivUiStatus::Ok);
            }
            assert_eq!(divui_document_destroy(self.document), DivUiStatus::Ok);
        }
    }

    #[test]
    fn c_edit_command_header_and_payload_validation_is_nonmutating() {
        assert_eq!(size_of::<DivUiEditCommandV1>(), 32);
        assert_eq!(std::mem::align_of::<DivUiEditCommandV1>(), 4);
        let fixture = Fixture::new(23);
        fixture.set("á👩‍💻z");
        assert_eq!(
            divui_element_edit_text_v1(fixture.input, ptr::null()),
            DivUiStatus::InvalidArgument
        );
        let short_size = 4_u32;
        assert_eq!(
            divui_element_edit_text_v1(fixture.input, (&short_size as *const u32).cast()),
            DivUiStatus::InvalidArgument
        );
        let short_header = [8_u32, DIVUI_ABI_VERSION];
        assert_eq!(
            divui_element_edit_text_v1(fixture.input, short_header.as_ptr().cast()),
            DivUiStatus::InvalidArgument
        );
        let mut raw = command(DIVUI_EDIT_DELETE);
        raw.abi_version = 1;
        assert_eq!(
            divui_element_edit_text_v1(fixture.input, &raw),
            DivUiStatus::AbiMismatch
        );
        for (kind, direction, unit, extend, reserved) in [
            (99, 0, 0, 0, [0, 0]),
            (DIVUI_EDIT_MOVE, 99, 0, 0, [0, 0]),
            (DIVUI_EDIT_DELETE, 0, 99, 0, [0, 0]),
            (DIVUI_EDIT_MOVE, 0, 0, 2, [0, 0]),
            (DIVUI_EDIT_DELETE, 0, 0, 1, [0, 0]),
            (DIVUI_EDIT_SELECT_ALL, 1, 0, 0, [0, 0]),
            (DIVUI_EDIT_UNDO, 0, 1, 0, [0, 0]),
            (DIVUI_EDIT_REDO, 0, 0, 1, [0, 0]),
            (DIVUI_EDIT_DELETE, 0, 0, 0, [1, 0]),
            (DIVUI_EDIT_DELETE, 0, 0, 0, [0, 1]),
        ] {
            raw = command(kind);
            raw.direction = direction;
            raw.unit = unit;
            raw.extend_selection = extend;
            raw.reserved = reserved;
            assert_eq!(
                divui_element_edit_text_v1(fixture.input, &raw),
                DivUiStatus::InvalidArgument
            );
        }
        assert_eq!(
            fixture.native().control_value().unwrap().as_deref(),
            Some("á👩‍💻z")
        );
        assert_eq!(fixture.native().selection().unwrap(), Some((15, 15)));
        #[repr(C)]
        struct Extended {
            base: DivUiEditCommandV1,
            future: [u32; 2],
        }
        let mut extended = Extended {
            base: command(DIVUI_EDIT_SELECT_ALL),
            future: [0; 2],
        };
        extended.base.struct_size = size_of::<Extended>() as u32;
        assert_eq!(
            divui_element_edit_text_v1(fixture.input, &extended.base),
            DivUiStatus::Ok
        );
        assert_eq!(fixture.native().selection().unwrap(), Some((0, 15)));
    }

    struct Callback {
        sibling: *mut DivUiElement,
        values: Vec<String>,
        failed: bool,
    }

    unsafe extern "C" fn observe(event: *mut DivUiEvent, user_data: *mut c_void) {
        // SAFETY: both pointers stay live during synchronous test dispatch.
        let event = unsafe { &*event };
        let data = unsafe { &mut *user_data.cast::<Callback>() };
        let mut bytes = [0_u8; 64];
        let mut length = 0;
        if event.event_type != 16
            || event.target != event.current_target
            || divui_element_copy_control_value(
                event.target,
                bytes.as_mut_ptr(),
                bytes.len(),
                &mut length,
            ) != DivUiStatus::Ok
            || divui_element_set_text(data.sibling, text("callback updated sibling"))
                != DivUiStatus::Ok
        {
            data.failed = true;
            return;
        }
        data.values
            .push(String::from_utf8(bytes[..length].to_vec()).unwrap());
    }

    #[test]
    fn c_and_public_rust_editing_share_history_and_reentrant_input_callbacks() {
        for tag in [23, 31] {
            let fixture = Fixture::new(tag);
            fixture.set("á👩‍💻z");
            let mut data = Callback {
                sibling: fixture.sibling,
                values: Vec::new(),
                failed: false,
            };
            let mut listener = ptr::null_mut();
            assert_eq!(
                divui_element_add_event_listener(
                    fixture.input,
                    16,
                    0,
                    Some(observe),
                    (&mut data as *mut Callback).cast(),
                    &mut listener
                ),
                DivUiStatus::Ok
            );
            assert_eq!(
                divui_element_edit_text_v1(fixture.input, &command(DIVUI_EDIT_DELETE)),
                DivUiStatus::Ok
            );
            let native = fixture.native();
            native.edit_text(EditCommand::Undo).unwrap();
            assert_eq!(
                divui_element_edit_text_v1(fixture.input, &command(DIVUI_EDIT_REDO)),
                DivUiStatus::Ok
            );
            assert_eq!(data.values, ["á👩‍💻", "á👩‍💻z", "á👩‍💻"]);
            assert!(!data.failed);
            assert_eq!(
                divui_element_edit_text_v1(fixture.input, &command(DIVUI_EDIT_SELECT_ALL)),
                DivUiStatus::Ok
            );
            let mut movement = command(DIVUI_EDIT_MOVE);
            movement.direction = DIVUI_TEXT_FORWARD;
            movement.unit = DIVUI_TEXT_DOCUMENT;
            assert_eq!(
                divui_element_edit_text_v1(fixture.input, &movement),
                DivUiStatus::Ok
            );
            assert_eq!(native.selection().unwrap(), Some((14, 14)));
            assert_eq!(data.values.len(), 3);
            assert_eq!(divui_listener_destroy(listener), DivUiStatus::Ok);
        }
    }

    #[test]
    fn c_editing_units_directions_and_extensions_match_public_rust_state() {
        for tag in [23, 31] {
            let fixture = Fixture::new(tag);
            let state = document(fixture.document as usize).unwrap();
            let rust =
                div::Element::create(&state.native, if tag == 23 { "input" } else { "textarea" })
                    .unwrap();
            for direction in [DIVUI_TEXT_BACKWARD, DIVUI_TEXT_FORWARD] {
                for unit in [
                    DIVUI_TEXT_GRAPHEME,
                    DIVUI_TEXT_WORD,
                    DIVUI_TEXT_LINE,
                    DIVUI_TEXT_DOCUMENT,
                ] {
                    for (kind, extend) in [
                        (DIVUI_EDIT_MOVE, 0),
                        (DIVUI_EDIT_MOVE, 1),
                        (DIVUI_EDIT_DELETE, 0),
                    ] {
                        fixture.set("á👩‍💻 word\nlast");
                        let value = fixture.native().control_value().unwrap().unwrap();
                        rust.set_control_value(&value).unwrap();
                        // UTF-8 offset 14 is immediately after the emoji cluster.
                        assert_eq!(
                            divui_element_set_selection(fixture.input, 14, 14),
                            DivUiStatus::Ok
                        );
                        rust.set_selection(14, 14).unwrap();
                        let mut raw = command(kind);
                        raw.direction = direction;
                        raw.unit = unit;
                        raw.extend_selection = extend;
                        let rust_direction = if direction == DIVUI_TEXT_BACKWARD {
                            TextDirection::Backward
                        } else {
                            TextDirection::Forward
                        };
                        let rust_unit = match unit {
                            0 => TextUnit::Grapheme,
                            1 => TextUnit::Word,
                            2 => TextUnit::Line,
                            _ => TextUnit::Document,
                        };
                        let typed = if kind == DIVUI_EDIT_MOVE {
                            EditCommand::Move {
                                direction: rust_direction,
                                unit: rust_unit,
                                extend: extend != 0,
                            }
                        } else {
                            EditCommand::Delete {
                                direction: rust_direction,
                                unit: rust_unit,
                            }
                        };
                        assert_eq!(
                            divui_element_edit_text_v1(fixture.input, &raw),
                            DivUiStatus::Ok
                        );
                        rust.edit_text(typed).unwrap();
                        assert_eq!(
                            fixture.native().control_value().unwrap(),
                            rust.control_value().unwrap()
                        );
                        assert_eq!(
                            fixture.native().selection().unwrap(),
                            rust.selection().unwrap()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn c_editing_enforces_readonly_disabled_thread_borrow_and_handle_rules() {
        let fixture = Fixture::new(23);
        fixture.set("á👩‍💻z");
        assert_eq!(
            divui_element_set_attribute(fixture.input, text("readonly"), text("")),
            DivUiStatus::Ok
        );
        assert_eq!(
            divui_element_edit_text_v1(fixture.input, &command(DIVUI_EDIT_SELECT_ALL)),
            DivUiStatus::Ok
        );
        for kind in [DIVUI_EDIT_DELETE, DIVUI_EDIT_UNDO, DIVUI_EDIT_REDO] {
            assert_eq!(
                divui_element_edit_text_v1(fixture.input, &command(kind)),
                DivUiStatus::InvalidState
            );
        }
        assert_eq!(
            divui_element_set_attribute(fixture.input, text("disabled"), text("")),
            DivUiStatus::Ok
        );
        for kind in [
            DIVUI_EDIT_MOVE,
            DIVUI_EDIT_DELETE,
            DIVUI_EDIT_SELECT_ALL,
            DIVUI_EDIT_UNDO,
            DIVUI_EDIT_REDO,
        ] {
            assert_eq!(
                divui_element_edit_text_v1(fixture.input, &command(kind)),
                DivUiStatus::InvalidState
            );
        }
        assert_eq!(
            divui_element_edit_text_v1(fixture.sibling, &command(DIVUI_EDIT_SELECT_ALL)),
            DivUiStatus::InvalidState
        );
        let state = document(fixture.document as usize).unwrap();
        let borrow = state.engine.borrow_mut();
        assert_eq!(
            divui_element_edit_text_v1(fixture.input, &command(DIVUI_EDIT_SELECT_ALL)),
            DivUiStatus::Reentrant
        );
        drop(borrow);
        let address = fixture.input as usize;
        assert_eq!(
            std::thread::spawn(move || divui_element_edit_text_v1(
                address as *mut DivUiElement,
                &command(DIVUI_EDIT_SELECT_ALL)
            ))
            .join()
            .unwrap(),
            DivUiStatus::WrongThread
        );
        assert_eq!(divui_element_remove(fixture.input), DivUiStatus::Ok);
        assert_eq!(
            divui_element_edit_text_v1(fixture.input, &command(DIVUI_EDIT_SELECT_ALL)),
            DivUiStatus::StaleHandle
        );
        assert_eq!(
            divui_element_edit_text_v1(ptr::null_mut(), &command(DIVUI_EDIT_SELECT_ALL)),
            DivUiStatus::InvalidArgument
        );
    }

    #[test]
    fn c_editing_contains_native_listener_panics_and_releases_event_borrows() {
        let fixture = Fixture::new(23);
        fixture.set("á👩‍💻z");
        let native = fixture.native();
        native
            .on("input", |_| {
                panic!("native callback panic containment guard")
            })
            .unwrap();
        assert_eq!(
            divui_element_edit_text_v1(fixture.input, &command(DIVUI_EDIT_DELETE)),
            DivUiStatus::Internal
        );
        assert_eq!(native.control_value().unwrap().as_deref(), Some("á👩‍💻"));
        assert_eq!(
            divui_element_edit_text_v1(fixture.input, &command(DIVUI_EDIT_SELECT_ALL)),
            DivUiStatus::Ok
        );
        assert_eq!(native.selection().unwrap(), Some((0, 14)));
    }
}
