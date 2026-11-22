//! # Div
//!
//! Safe, direct Rust framework over the pure-Rust Div retained engine.
//! Application interaction uses Rust callbacks and native [`Document`] and
//! [`Element`] methods. Browser-like element behavior needed by applications
//! belongs in this public native Rust API. Div does not execute JavaScript
//! or expose browser script bindings.
//!
//! ```no_run
//! use div::prelude::*;
//!
//! # fn main() -> Result<(), Error> {
//! let count = create_signal(0_i32);
//! let app = App::builder()
//!     .title("Div")
//!     .size(LogicalSize::new(800.0, 600.0))
//!     .backend(BackendPreference::Auto)
//!     .build()?;
//!
//! app.run(move || view! {
//!     <button
//!         style:display={Display::Flex}
//!         style:padding="8px 16px"
//!         on:click={move |_| count.update(|value| *value += 1)}
//!     >
//!         {count.get()}
//!     </button>
//! })?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Reactive primitives
//!
//! | Primitive | Purpose |
//! |-----------|---------|
//! | [`Signal`] | Readable/writable reactive value |
//! | [`Memo`] | Cached derived value |
//! | [`create_effect`] | Side-effect that re-runs on dependency changes |
//! | [`batch`] | Defer effect execution across multiple updates |
//! | [`create_scope`] | Group effects for batch disposal |
//! | [`dispose_scope`] | Tear down a scope and its children |
//! | [`on_cleanup`] | Register a cleanup callback in the current scope |
//!
//! ## Native document and element handles
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`Document`] | Native retained document and rendering context |
//! | [`Element`] | A node in the retained document tree |
//! | [`TextNode`] | Native text data that can be created, edited, attached, and moved |
//! | [`AnimationFrameHandle`] | A callback from [`Document::request_animation_frame`], run at the next frame |
//! | [`ListenerHandle`] | One event listener from [`Element::on`] or [`Element::on_with`], removable on its own |
//! | [`ListenerOptions`] | `capture`, `once` and `passive`, as the web's `AddEventListenerOptions` |
//! | [`WheelDeltaMode`] | The unit of a wheel event's deltas, as the web's `WheelEvent.deltaMode` |
//! | [`PointerInput`] | One input from a mouse, pen or touch, for [`Document::dispatch_pointer_input`] |
//! | [`PointerInfo`] | A pointer event's data, as the web's `PointerEvent`, from [`Event::pointer_info`] |
//! | [`MouseInfo`] | A mouse event's data, as the web's `MouseEvent`, from [`Event::mouse_info`] |
//! | [`PointerId`] | A pointer's id; the mouse is [`PointerId::MOUSE`], as in Chromium |
//! | [`Buttons`] | The pressed buttons, as the web's `MouseEvent.buttons` bits |
//! | [`ClickCounter`] | Counts repeated clicks for `detail`, by Chromium's Windows or Linux rule |
//! | [`DoubleClickSettings`] | A platform's double-click time and box, for [`ClickCounter`] |
//! | [`TapCounter`] | Counts repeated touch taps, as Chromium's gesture detector |
//! | [`ResizeObserver`] | Calls back when observed elements' boxes change size |
//! | [`ReportedError`] | An error a document reports to [`Document::on_error`] |
//!
//! ## View system
//!
//! | Type / Macro | Purpose |
//! |--------------|---------|
//! | [`view!`] | JSX-like declarative UI macro |
//! | [`style!`] | CSS declaration block compiled to a typed [`Style`] |
//! | [`component`] | Attribute macro to define a component |
//! | [`ViewNode`] | A renderable node in the UI tree |
//! | [`IntoView`] | Trait for converting values into view nodes |
//! | [`mount_view`] | Mount a view node onto a parent element |
//! | [`with_document`] | Set the render context document |
//! | [`current_document`] | Access the current render context document |

extern crate self as div;

// ─── Reactive runtime modules ───────────────────────────────

pub mod effect;
pub mod runtime;
pub mod scope;
pub mod signal;

// ─── DOM wrapper modules ────────────────────────────────────

mod animation_frame;
pub use animation_frame::AnimationFrameHandle;
mod reported_error;
pub use reported_error::ReportedError;
mod resize_observer;
pub use resize_observer::{
    ResizeObserver, ResizeObserverBox, ResizeObserverEntry, ResizeObserverSize,
};
mod animation_timeline;
pub use animation_timeline::AnimationTimelineBinding;

pub mod click_count;
pub mod document;
pub mod element;
pub mod events;
mod keyboard;
pub mod pointer;
pub mod scroll_button;
pub mod style;
mod style_preset;
pub mod text_node;

// ─── View system modules ────────────────────────────────────

#[doc(hidden)]
#[path = "view_child.rs"]
pub mod __view_child;
pub mod context;
pub mod renderer;
pub mod view_node;

mod generated_style_setters;

// ─── Application shell ─────────────────────────────────────

pub mod app;
mod app_spec;
pub mod clipboard;
#[cfg(all(feature = "dialog", any(target_os = "linux", target_os = "windows")))]
pub mod dialog;
#[cfg(feature = "http")]
pub mod http;
#[cfg(feature = "menu")]
pub mod menu;
#[cfg(feature = "menu")]
mod menu_bar;
#[cfg(feature = "async")]
mod resource;
#[cfg(feature = "async")]
pub mod task;
#[cfg(feature = "testing")]
pub mod testing;
pub mod window;

/// Convenience prelude for common imports.
pub mod prelude;

// ─── Re-exports: reactive primitives ────────────────────────

pub use effect::{batch, create_effect};
#[cfg(feature = "async")]
pub use resource::{create_resource, Resource};
pub use runtime::{EffectId, ScopeId, SignalId};
pub use scope::{create_scope, dispose_scope, on_cleanup};
#[cfg(feature = "async")]
pub use signal::SignalSender;
pub use signal::{create_memo, create_signal, Memo, Signal};

// ─── Re-exports: DOM wrappers ───────────────────────────────

pub use click_count::{ClickCountRule, ClickCounter, ClickInput, DoubleClickSettings, TapCounter};
pub use div_dom::ElementTag;
pub use document::Document;
pub use element::{Element, WeakElement};
pub use events::{
    Event, EventPhase, InputEventInfo, InputType, KeyEventType, ListenerHandle, ListenerOptions,
    Modifiers, MouseButton, MouseEventType, WheelDeltaMode,
};
pub use keyboard::Key;
pub use pointer::{
    Buttons, MouseInfo, Point, PointerId, PointerInfo, PointerInput, PointerInputKind, PointerType,
};
pub use scroll_button::{
    ScrollButton, ScrollButtonAvailability, ScrollButtonDirection, ScrollButtonState,
    ScrollSnapStrictness,
};
pub use style::{Bitmap, Error, Rect};
pub use style_preset::{StylePreset, UserAgentStyle};
pub use text_node::{TextNode, WeakTextNode};

/// Canonical typed style values shared with the engine, macro, and C schema.
pub mod typed_style {
    pub use div_style::*;
}
pub use div_style::{ComputedStyle, Style, StyleProperty, StyleValue};
pub use div_text::{
    AppliedPlatformFontSettings, AppliedSkFontSettings, FontFaceSourceInfo, FontInstantiation,
    FontProviderConstruction, FontRenderHintStyle, FontRenderPreference, FontRenderQueryIntent,
    FontRenderStyle, FontSelectionOrigin, FontSelectionSource,
};
pub use div_text::{
    FontAxisRange, FontCollection, FontCollectionError, FontCollectionStats, FontContainerFormat,
    FontFaceDescriptor, FontFaceHandle, FontFaceInfo, FontFeatureDefault, FontMetricOverrides,
    FontPaletteBase, FontPaletteEntryOverride, FontPaletteHandle, FontPaletteValuesDescriptor,
    FontStyleRange, FontUnicodeRange, HyphenationDictionaryHandle, HyphenationRegistry,
    HyphenationRegistryError,
};

// ─── Re-exports: view system ────────────────────────────────

pub use context::{current_document, with_document};
pub use renderer::{DynChild, For, Show};
#[doc(hidden)]
pub use view_node::__forward_listener;
pub use view_node::{mount_view, IntoView, ViewNode};

// ─── Re-exports: application shell ─────────────────────────

pub use app::{
    App, AppBuilder, AppExitHandle, BackendPreference, HeadlessApp, LogicalSize, RenderOptions,
    WindowOptions,
};
pub use app_spec::AppSpec;
pub use div_engine::{
    AccessibilityAction, AccessibilityLive, AccessibilityNode, AccessibilityNodeId,
    AccessibilityPlatformAction, AccessibilityRelation, AccessibilityRole,
    AccessibilityScrollDirection, AccessibilityTreeUpdate, AnimationAttachmentSnapshot,
    AnimationEvent, AnimationEventKind, AnimationId, AnimationResolvedTimelineRanges,
    AnimationState, AnimationTimelineSnapshot, AnimationTimelineSource, ControlAdjustment,
    EditCommand, EngineError, EngineOptions, FocusOrigin, PointerEventKind, RangeSelectionMode,
    RasterBackend, RasterConfiguration, RasterPixelGeometry, ScrollAlignment, ScrollAnimationId,
    ScrollIntoViewContainer, ScrollIntoViewOptions, ScrollMetrics, SelectionDirection,
    TextDirection, TextEdging, TextHinting, TextRasterConfiguration, TextUnit, ViewportAuthority,
    ViewportMetrics, ViewportMetricsError,
};
pub use div_engine::{StyleSheetError, StyleSheetId};
#[cfg(any(
    all(feature = "linux", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
pub use div_platform::{KeyboardInput, PlatformEvent};

// ─── Re-exports: proc macros ────────────────────────────────

pub use div_macros::{component, style, stylesheet, view};

#[cfg(test)]
mod form_association_tests;
#[cfg(test)]
mod tests;
