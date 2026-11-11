//! Normalized native wheel input. The retained Document owns dispatch/defaults.
use super::*;
use std::mem::align_of;

// SAFETY CONTRACT: the document is a live owning-thread handle. The descriptor
// has an initialized versioned header and readable declared extent. It remains
// immutable until this call returns; no caller pointer is retained.
#[no_mangle]
pub extern "C" fn divui_document_dispatch_wheel_input_v1(
    document_handle: *mut DivUiDocument,
    wheel: *const DivUiEvent,
) -> DivUiStatus {
    ffi(|| {
        let state = document(document_handle as usize)?;
        // Reject alignment/address failures before reading even the header.
        if wheel.is_null()
            || (wheel as usize) % align_of::<DivUiEvent>() != 0
            || (wheel as usize)
                .checked_add(size_of::<DivUiEvent>())
                .is_none()
        {
            return Err(invalid(
                "wheel descriptor pointer, alignment or extent is invalid",
            ));
        }
        // SAFETY: the caller supplies the initialized header/readable extent;
        // the shared reader checks size/version before copying the full prefix.
        let wheel = unsafe { easing_curve::read_input(wheel, None) }?;
        validate_event(&wheel)?;
        if wheel.event_type != 5 || wheel.flags != 0 {
            return Err(invalid(
                "normalized wheel input requires Wheel and zero flags",
            ));
        }
        // All used operands are copied scalars. Other generic descriptor fields
        // are not wheel inputs. The native route constructs its own Event and
        // timestamp, releases borrows before callbacks, and honors cancellation.
        state
            .native
            .dispatch_wheel_event(
                wheel.x,
                wheel.y,
                wheel.delta_x,
                wheel.delta_y,
                div::Modifiers(wheel.modifiers),
            )
            .map_err(native_app::native_error)
    })
}
