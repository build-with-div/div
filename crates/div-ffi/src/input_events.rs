//! Owned input metadata for callbacks through the shared native event pipeline.
use super::*;

struct ActiveInputEvent {
    address: usize,
    document: std::rc::Weak<DocumentState>,
    native: div::Event,
}
thread_local! {
    static ACTIVE_INPUT_EVENTS: RefCell<Vec<ActiveInputEvent>> = const { RefCell::new(Vec::new()) };
}
pub(crate) struct InputEventScope {
    address: usize,
}
impl InputEventScope {
    pub(crate) fn new(
        state: &Rc<DocumentState>,
        native: &div::Event,
        event: &DivUiEvent,
    ) -> Option<Self> {
        native.input_info()?;
        let address = event as *const DivUiEvent as usize;
        ACTIVE_INPUT_EVENTS.with(|events| {
            events.borrow_mut().push(ActiveInputEvent {
                address,
                document: Rc::downgrade(state),
                native: native.clone(),
            })
        });
        Some(Self { address })
    }
}
impl Drop for InputEventScope {
    fn drop(&mut self) {
        ACTIVE_INPUT_EVENTS.with(|events| {
            let removed = events.borrow_mut().pop();
            debug_assert!(removed.is_some_and(|event| event.address == self.address));
        });
    }
}
// SAFETY CONTRACT: out_info has a readable versioned header and writable
// storage for its declared current size. event is checked by address only.
#[no_mangle]
pub extern "C" fn divui_event_input_info_v1(
    event: *const DivUiEvent,
    out_info: *mut DivUiInputEventInfoV1,
) -> DivUiStatus {
    ffi(|| {
        if out_info.is_null() {
            return Err(invalid("input event info output is null"));
        }
        // SAFETY: caller provides a readable size field.
        let struct_size = unsafe { ptr::read(out_info.cast::<u32>()) };
        if struct_size < 2 * size_of::<u32>() as u32 {
            return Err(invalid("input event info header is too small"));
        }
        // SAFETY: declared header covers the ABI version.
        let abi_version = unsafe { ptr::read(out_info.cast::<u32>().add(1)) };
        check_header(struct_size, abi_version, size_of::<DivUiInputEventInfoV1>())?;
        // SAFETY: validated current-version storage includes the data field.
        if !unsafe { ptr::read(ptr::addr_of!((*out_info).data)) }.is_null() {
            return Err(invalid(
                "destroy previous input data buffer before reusing output",
            ));
        }
        let context = ACTIVE_INPUT_EVENTS.with(|events| {
            events
                .borrow()
                .iter()
                .rev()
                .find(|active| active.address == event as usize)
                .map(|active| (active.document.clone(), active.native.clone()))
        });
        let (document, native) = context.ok_or_else(|| {
            ApiError::new(
                DivUiStatus::InvalidState,
                "no active native input callback for event",
            )
        })?;
        let _document = document.upgrade().ok_or_else(|| {
            ApiError::new(
                DivUiStatus::InvalidState,
                "input event document was destroyed",
            )
        })?;
        let info = native.input_info().ok_or_else(|| {
            ApiError::new(
                DivUiStatus::InvalidState,
                "callback has no native input metadata",
            )
        })?;
        let input_type = match info.input_type() {
            div::InputType::InsertText => 1,
            div::InputType::InsertLineBreak => 2,
            div::InputType::DeleteContentBackward => 3,
            div::InputType::DeleteContentForward => 4,
            div::InputType::HistoryUndo => 5,
            div::InputType::HistoryRedo => 6,
            div::InputType::DeleteWordBackward => 7,
            div::InputType::DeleteWordForward => 8,
            div::InputType::DeleteByCut => 9,
            div::InputType::InsertFromPaste => 10,
            _ => {
                return Err(ApiError::new(
                    DivUiStatus::InvalidState,
                    "input type has no version-one C tag",
                ))
            }
        };
        let data = if let Some(text) = info.data() {
            register(LocalHandle::Buffer(Rc::new(text.as_bytes().to_vec())))? as *mut DivUiBuffer
        } else {
            ptr::null_mut()
        };
        // SAFETY: validated writable storage; no fallible work follows the
        // owned buffer registration, so errors cannot leak its ownership.
        unsafe {
            ptr::write(
                out_info,
                DivUiInputEventInfoV1 {
                    struct_size: size_of::<DivUiInputEventInfoV1>() as u32,
                    abi_version: DIVUI_ABI_VERSION,
                    input_type,
                    has_data: u32::from(info.data().is_some()),
                    is_composing: u32::from(info.is_composing()),
                    bubbles: u32::from(native.bubbles()),
                    cancelable: u32::from(native.cancelable()),
                    data,
                },
            );
        }
        Ok(())
    })
}
