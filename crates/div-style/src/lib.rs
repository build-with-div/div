//! Typed CSS style properties — extracted from Blink's `ComputedStyle`.
//!
//! Properties are set programmatically through typed Rust APIs. Native literal
//! helpers admit documented value subsets; they do not parse stylesheets.
//! Every enum, every initial value, every type is extracted character-by-character
//! from Blink's generated `computed_style_base.h` and `computed_style_constants.h`.

mod animation;
mod animation_value;
mod authored_style;
mod color;
mod color_function;
mod computed;
#[cfg(feature = "constructor-tokens")]
#[doc(hidden)]
pub mod constructor_tokens;
mod css_tokens;
#[doc(hidden)]
pub mod declaration_block;
mod enums;
mod font_types;
mod inline_style;
mod layout_systems;
pub mod math;
#[doc(hidden)]
pub mod media_query;
mod property;
mod radius_axes;
pub mod selector;
#[cfg(feature = "constructor-tokens")]
#[doc(hidden)]
pub mod selector_parser;
pub mod stylesheet;
#[cfg(feature = "constructor-tokens")]
#[doc(hidden)]
pub mod stylesheet_parser;
mod transform_composition;
mod transform_literal;
mod typography;

pub use animation::*;
pub use animation_value::*;
pub use authored_style::*;
pub use color::{Color, StyleColor};
pub use computed::{
    AspectRatio, BackgroundLayer, BackgroundPosition, BackgroundRepeat, BackgroundSize,
    BlockEllipsis, BorderImage, BorderImageLength, BorderImageRepeat, BoxShadow, ComputedStyle,
    ComputedStyleBuilder, ComputedStyleFields, ConicGradient, CounterOperation, CounterStyle,
    CssImage, CssLinearGradient, GeneratedContentItem, GradientColorSpace, GradientStop,
    GradientStopPosition, ImageResourceId, LineClamp, LinearGradient, LinearGradientStop,
    PositionArea, QuotePair, RadialGradient, RadialGradientShape, RadialGradientSize,
    WebkitBoxOrient,
};
pub use div_geometry::Length;
pub use enums::*;
pub use font_types::*;
pub use inline_style::*;
pub use layout_systems::*;
pub use property::*;
pub use radius_axes::*;
pub use transform_composition::*;
pub use typography::*;
