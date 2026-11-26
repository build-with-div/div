//! Media queries (Media Queries Level 4), parsed and evaluated as Chromium
//! 147 does (RFC 0001, RFC 0014).
//!
//! The grammar follows Blink's `MediaQueryParser` and `ConditionalParser`
//! (`core/css/parser/media_query_parser.cc`, `conditional_parser.cc`) and
//! the value rules of `MediaQueryExpValue::Consume`
//! (`core/css/media_query_exp.cc`). Evaluation follows
//! `MediaQueryEvaluator` (`core/css/media_query_evaluator.cc`), with its
//! three-valued logic: a `<general-enclosed>` term is unknown, and a query
//! whose result is unknown does not match. All are Chromium 147, BSD.
//!
//! [`parse_media_query_list`] is browser-compatible: an invalid query
//! becomes `not all` and an unrecognised term is unknown, as in Chromium. It
//! also lists each such problem, so `stylesheet!` can reject them at
//! compile time, as RFC 0001 requires of everything Div does not evaluate.

use crate::css_tokens::{component_values, ComponentValue, Token};

/// A comma-separated list of media queries. It matches when any query
/// matches; an empty list matches. The `Data` suffix leaves the web's name
/// for `div::MediaQueryList`, the handle `Document::match_media` returns.
#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
#[derive(Debug, Clone, PartialEq)]
pub struct MediaQueryListData {
    pub queries: Vec<MediaQuery>,
}

/// `[not | only]? <media-type> [and <condition>]?`, or a condition alone,
/// whose media type is `all`.
#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
#[derive(Debug, Clone, PartialEq)]
pub struct MediaQuery {
    pub restrictor: MediaRestrictor,
    pub media_type: MediaType,
    pub condition: Option<MediaCondition>,
}

#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaRestrictor {
    None,
    Not,
    Only,
}

/// A media type. Div renders to a screen, so `all` and `screen` match;
/// `print` and every other type never match.
#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaType {
    All,
    Screen,
    Print,
    /// Any other identifier, lowercased.
    Other(String),
}

#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
#[derive(Debug, Clone, PartialEq)]
pub enum MediaCondition {
    Feature(MediaFeature),
    Not(Box<MediaCondition>),
    And(Vec<MediaCondition>),
    Or(Vec<MediaCondition>),
    /// A parenthesized condition, `(not (hover))` or `((width > 1px))`.
    /// Blink keeps the grouping as a nested node, and `matchMedia().media`
    /// serializes it; it evaluates as its inner condition.
    Nested(Box<MediaCondition>),
    /// `<general-enclosed>`: a parenthesized or functional term Chromium
    /// does not recognise. It evaluates to unknown.
    Unknown(String),
}

/// One media feature test.
#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
#[derive(Debug, Clone, PartialEq)]
pub struct MediaFeature {
    pub name: MediaFeatureName,
    /// `min-` or `max-`, which make a plain test `>=` or `<=`.
    pub prefix: Option<RangePrefix>,
    pub test: FeatureTest,
}

#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangePrefix {
    Min,
    Max,
}

#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
#[derive(Debug, Clone, PartialEq)]
pub enum FeatureTest {
    /// `(name)`.
    Boolean,
    /// `(name: value)`.
    Plain(FeatureValue),
    /// `(value op name)`, `(name op value)` or `(value op name op value)`.
    /// The left bound reads `value op name`; the right `name op value`.
    Range {
        left: Option<(FeatureValue, Comparison)>,
        right: Option<(Comparison, FeatureValue)>,
    },
}

#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Comparison {
    /// `a op b` as `b reversed(op) a`.
    fn reversed(self) -> Self {
        match self {
            Self::Eq => Self::Eq,
            Self::Lt => Self::Gt,
            Self::Le => Self::Ge,
            Self::Gt => Self::Lt,
            Self::Ge => Self::Le,
        }
    }

    fn is_less(self) -> bool {
        matches!(self, Self::Lt | Self::Le)
    }

    fn is_greater(self) -> bool {
        matches!(self, Self::Gt | Self::Ge)
    }
}

/// The media features Div evaluates.
#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaFeatureName {
    Width,
    Height,
    AspectRatio,
    Orientation,
    Resolution,
    /// `-webkit-device-pixel-ratio`.
    WebkitDevicePixelRatio,
    PrefersColorScheme,
    PrefersReducedMotion,
    Hover,
    AnyHover,
    Pointer,
    AnyPointer,
    ForcedColors,
}

impl MediaFeatureName {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "width" => Self::Width,
            "height" => Self::Height,
            "aspect-ratio" => Self::AspectRatio,
            "orientation" => Self::Orientation,
            "resolution" => Self::Resolution,
            "-webkit-device-pixel-ratio" => Self::WebkitDevicePixelRatio,
            "prefers-color-scheme" => Self::PrefersColorScheme,
            "prefers-reduced-motion" => Self::PrefersReducedMotion,
            "hover" => Self::Hover,
            "any-hover" => Self::AnyHover,
            "pointer" => Self::Pointer,
            "any-pointer" => Self::AnyPointer,
            "forced-colors" => Self::ForcedColors,
            _ => return None,
        })
    }

    /// Range features take `min-`/`max-` prefixes and the range syntax
    /// (Blink's `IsRangeTypeFeature`).
    fn is_range(self) -> bool {
        matches!(
            self,
            Self::Width
                | Self::Height
                | Self::AspectRatio
                | Self::Resolution
                | Self::WebkitDevicePixelRatio
        )
    }
}

/// Media features Chromium 147 evaluates and Div does not yet. A query
/// naming one is rejected by `stylesheet!`; at run time it is unknown.
const UNSUPPORTED_FEATURES: &[&str] = &[
    "color",
    "color-index",
    "monochrome",
    "grid",
    "device-width",
    "device-height",
    "device-aspect-ratio",
    "transform-3d",
    "display-mode",
    "scan",
    "color-gamut",
    "dynamic-range",
    "video-dynamic-range",
    "prefers-contrast",
    "prefers-reduced-data",
    "prefers-reduced-transparency",
    "inverted-colors",
    "overflow-inline",
    "overflow-block",
    "update",
    "scripting",
];

#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
#[derive(Debug, Clone, PartialEq)]
pub enum FeatureValue {
    Length { value: f64, unit: MediaLengthUnit },
    Number { value: f64, integer: bool },
    Ratio { numerator: f64, denominator: f64 },
    Resolution { value: f64, unit: ResolutionUnit },
    Keyword(MediaKeyword),
}

#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaLengthUnit {
    Px,
    Cm,
    Mm,
    Q,
    In,
    Pt,
    Pc,
    Em,
    Rem,
    Vw,
    Vh,
    Vmin,
    Vmax,
}

impl MediaLengthUnit {
    fn from_name(unit: &str) -> Option<Self> {
        Some(match unit.to_ascii_lowercase().as_str() {
            "px" => Self::Px,
            "cm" => Self::Cm,
            "mm" => Self::Mm,
            "q" => Self::Q,
            "in" => Self::In,
            "pt" => Self::Pt,
            "pc" => Self::Pc,
            "em" => Self::Em,
            "rem" => Self::Rem,
            "vw" => Self::Vw,
            "vh" => Self::Vh,
            "vmin" => Self::Vmin,
            "vmax" => Self::Vmax,
            _ => return None,
        })
    }
}

/// Length units Chromium accepts in media queries that Div does not
/// resolve yet: those that need the initial font's metrics, and the
/// small, large and dynamic viewport units.
const UNSUPPORTED_LENGTH_UNITS: &[&str] = &[
    "ex", "ch", "ic", "cap", "lh", "rlh", "rex", "rch", "ric", "rcap", "svw", "svh", "lvw", "lvh",
    "dvw", "dvh", "svi", "svb", "lvi", "lvb", "dvi", "dvb", "vi", "vb", "svmin", "svmax", "lvmin",
    "lvmax", "dvmin", "dvmax",
];

#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionUnit {
    Dppx,
    X,
    Dpi,
    Dpcm,
}

impl ResolutionUnit {
    fn from_name(unit: &str) -> Option<Self> {
        Some(match unit.to_ascii_lowercase().as_str() {
            "dppx" => Self::Dppx,
            "x" => Self::X,
            "dpi" => Self::Dpi,
            "dpcm" => Self::Dpcm,
            _ => return None,
        })
    }
}

#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKeyword {
    Portrait,
    Landscape,
    None,
    Hover,
    Coarse,
    Fine,
    Light,
    Dark,
    NoPreference,
    Reduce,
    Active,
}

impl MediaKeyword {
    /// The keyword, if it is valid for `feature` (Blink's
    /// `FeatureWithValidIdent`).
    fn for_feature(feature: MediaFeatureName, ident: &str) -> Option<Self> {
        use MediaFeatureName as F;
        let keyword = match ident.to_ascii_lowercase().as_str() {
            "portrait" => Self::Portrait,
            "landscape" => Self::Landscape,
            "none" => Self::None,
            "hover" => Self::Hover,
            "coarse" => Self::Coarse,
            "fine" => Self::Fine,
            "light" => Self::Light,
            "dark" => Self::Dark,
            "no-preference" => Self::NoPreference,
            "reduce" => Self::Reduce,
            "active" => Self::Active,
            _ => return None,
        };
        let valid = match feature {
            F::Orientation => matches!(keyword, Self::Portrait | Self::Landscape),
            F::Pointer | F::AnyPointer => matches!(keyword, Self::None | Self::Coarse | Self::Fine),
            F::Hover | F::AnyHover => matches!(keyword, Self::None | Self::Hover),
            F::PrefersColorScheme => matches!(keyword, Self::Light | Self::Dark),
            F::PrefersReducedMotion => matches!(keyword, Self::NoPreference | Self::Reduce),
            F::ForcedColors => matches!(keyword, Self::None | Self::Active),
            _ => false,
        };
        valid.then_some(keyword)
    }
}

/// The color scheme a document prefers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColorScheme {
    #[default]
    Light,
    Dark,
}

/// Whether a pointing device can hover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HoverCapability {
    None,
    Hover,
}

/// The accuracy of a pointing device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerAccuracy {
    None,
    Coarse,
    Fine,
}

/// What media queries are evaluated against (Blink's `MediaValues`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MediaValues {
    /// The viewport size in CSS pixels.
    pub width: f64,
    pub height: f64,
    pub device_pixel_ratio: f64,
    pub color_scheme: ColorScheme,
    pub reduced_motion: bool,
    /// The primary pointing device.
    pub hover: HoverCapability,
    pub pointer: PointerAccuracy,
    /// Every pointing device: `any-hover` and `any-pointer`.
    pub any_hover_none: bool,
    pub any_hover: bool,
    pub any_pointer_none: bool,
    pub any_pointer_coarse: bool,
    pub any_pointer_fine: bool,
    /// The initial font size, which `em` and `rem` resolve against.
    pub initial_font_size: f64,
}

impl MediaValues {
    /// A desktop screen with a mouse, light scheme, no reduced motion and
    /// a 16px initial font size.
    pub fn screen(width: f64, height: f64, device_pixel_ratio: f64) -> Self {
        Self {
            width,
            height,
            device_pixel_ratio,
            color_scheme: ColorScheme::Light,
            reduced_motion: false,
            hover: HoverCapability::Hover,
            pointer: PointerAccuracy::Fine,
            any_hover_none: false,
            any_hover: true,
            any_pointer_none: false,
            any_pointer_coarse: false,
            any_pointer_fine: true,
            initial_font_size: 16.0,
        }
    }
}

// ── Parsing ─────────────────────────────────────────────────────────────

/// A parsed media query list, and what Chromium accepts but treats as
/// `not all` or unknown, or Div does not evaluate yet.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedMediaQueryList {
    pub list: MediaQueryListData,
    pub problems: Vec<String>,
}

/// Parse a media query list as Chromium parses `@media` preludes and
/// `matchMedia()` text.
pub fn parse_media_query_list(text: &str) -> ParsedMediaQueryList {
    let values = component_values(text);
    let mut problems = Vec::new();
    let mut queries = Vec::new();
    let segments: Vec<&[ComponentValue]> = values
        .split(|value| matches!(value, ComponentValue::Token(Token::Comma)))
        .collect();
    // Blink: empty input is an empty list, which matches.
    if values.iter().all(ComponentValue::is_whitespace) {
        return ParsedMediaQueryList {
            list: MediaQueryListData { queries },
            problems,
        };
    }
    for segment in segments {
        let mut cursor = Cursor::new(segment);
        cursor.skip_whitespace();
        let mut notes = Vec::new();
        let query = consume_query(&mut cursor, &mut notes).filter(|_| {
            cursor.skip_whitespace();
            cursor.at_end()
        });
        match query {
            Some(query) => {
                if let MediaType::Other(name) = &query.media_type {
                    problems.push(format!("`{name}` is not a media type Div renders to"));
                }
                collect_unknown(query.condition.as_ref(), &mut notes, &mut problems);
                queries.push(query);
            }
            None => {
                let text = serialize(segment).trim().to_owned();
                let reason = notes
                    .pop()
                    .map(|note| format!(": {note}"))
                    .unwrap_or_default();
                problems.push(format!("`{text}` is not a valid media query{reason}"));
                queries.push(MediaQuery::not_all());
            }
        }
    }
    ParsedMediaQueryList {
        list: MediaQueryListData { queries },
        problems,
    }
}

fn collect_unknown(
    condition: Option<&MediaCondition>,
    notes: &mut Vec<String>,
    problems: &mut Vec<String>,
) {
    match condition {
        Some(MediaCondition::Unknown(text)) => {
            let reason = notes
                .iter()
                .find(|note| note.starts_with(&format!("`{text}`")))
                .cloned();
            problems.push(
                reason.unwrap_or_else(|| format!("`{text}` is not a valid media feature test")),
            );
        }
        Some(MediaCondition::Not(inner) | MediaCondition::Nested(inner)) => {
            collect_unknown(Some(inner), notes, problems)
        }
        Some(MediaCondition::And(list) | MediaCondition::Or(list)) => {
            for condition in list {
                collect_unknown(Some(condition), notes, problems);
            }
        }
        Some(MediaCondition::Feature(_)) | None => {}
    }
}

impl MediaQuery {
    /// `not all`, which an invalid query becomes.
    pub(crate) fn not_all() -> Self {
        Self {
            restrictor: MediaRestrictor::Not,
            media_type: MediaType::All,
            condition: None,
        }
    }
}

struct Cursor<'a> {
    values: &'a [ComponentValue],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(values: &'a [ComponentValue]) -> Self {
        Self { values, at: 0 }
    }

    fn peek(&self) -> Option<&'a ComponentValue> {
        self.values.get(self.at)
    }

    fn at_end(&self) -> bool {
        self.at >= self.values.len()
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(ComponentValue::is_whitespace) {
            self.at += 1;
        }
    }

    fn peek_ident(&self, word: &str) -> bool {
        self.peek()
            .and_then(ComponentValue::ident)
            .is_some_and(|ident| ident.eq_ignore_ascii_case(word))
    }

    /// Blink's `ConsumeIfIdent`: the identifier and the whitespace after it.
    fn consume_ident(&mut self, word: &str) -> bool {
        if self.peek_ident(word) {
            self.at += 1;
            self.skip_whitespace();
            true
        } else {
            false
        }
    }

    fn peek_delim(&self, delim: char) -> bool {
        matches!(self.peek(), Some(ComponentValue::Token(Token::Delim(c))) if *c == delim)
    }
}

fn consume_query(cursor: &mut Cursor, notes: &mut Vec<String>) -> Option<MediaQuery> {
    let start = cursor.at;
    let restrictor = if cursor.consume_ident("not") {
        MediaRestrictor::Not
    } else if cursor.consume_ident("only") {
        MediaRestrictor::Only
    } else {
        MediaRestrictor::None
    };
    if let Some(name) = cursor.peek().and_then(ComponentValue::ident) {
        let reserved = ["not", "and", "or", "only", "layer"]
            .iter()
            .any(|word| name.eq_ignore_ascii_case(word));
        if !reserved {
            cursor.at += 1;
            cursor.skip_whitespace();
            let media_type = match name.to_ascii_lowercase().as_str() {
                "all" => MediaType::All,
                "screen" => MediaType::Screen,
                "print" => MediaType::Print,
                other => MediaType::Other(other.to_owned()),
            };
            if !cursor.consume_ident("and") {
                return Some(MediaQuery {
                    restrictor,
                    media_type,
                    condition: None,
                });
            }
            let condition = consume_condition(cursor, false, notes)?;
            return Some(MediaQuery {
                restrictor,
                media_type,
                condition: Some(condition),
            });
        }
    }
    cursor.at = start;
    let condition = consume_condition(cursor, true, notes)?;
    Some(MediaQuery {
        restrictor: MediaRestrictor::None,
        media_type: MediaType::All,
        condition: Some(condition),
    })
}

/// Blink's `ConditionalParser::ConsumeCondition`.
fn consume_condition(
    cursor: &mut Cursor,
    allow_or: bool,
    notes: &mut Vec<String>,
) -> Option<MediaCondition> {
    if cursor.consume_ident("not") {
        return Some(MediaCondition::Not(Box::new(consume_inner(cursor, notes)?)));
    }
    let first = consume_inner(cursor, notes)?;
    if cursor.peek_ident("and") {
        let mut terms = vec![first];
        while cursor.consume_ident("and") {
            terms.push(consume_inner(cursor, notes)?);
        }
        return Some(MediaCondition::And(terms));
    }
    if allow_or && cursor.peek_ident("or") {
        let mut terms = vec![first];
        while cursor.consume_ident("or") {
            terms.push(consume_inner(cursor, notes)?);
        }
        return Some(MediaCondition::Or(terms));
    }
    Some(first)
}

/// Blink's `ConditionalParser::ConsumeInner`: a parenthesized feature or
/// condition, or `<general-enclosed>`.
fn consume_inner(cursor: &mut Cursor, notes: &mut Vec<String>) -> Option<MediaCondition> {
    let value = cursor.peek()?;
    let node = match value {
        ComponentValue::Block('(', inner) => {
            let mut inside = Cursor::new(inner);
            inside.skip_whitespace();
            let start = inside.at;
            let leaf = consume_feature(&mut inside);
            let node = match leaf {
                // Blink's `RestoringBlockGuard`: a feature followed by
                // anything else is `<general-enclosed>`.
                Ok(feature) => {
                    inside.skip_whitespace();
                    inside.at_end().then_some(MediaCondition::Feature(feature))
                }
                Err(note) => {
                    inside.at = start;
                    let condition = consume_condition(&mut inside, true, notes).filter(|_| {
                        inside.skip_whitespace();
                        inside.at_end()
                    });
                    if condition.is_none() {
                        if let Some(note) = note {
                            notes.push(note);
                        }
                    }
                    condition.map(|condition| MediaCondition::Nested(Box::new(condition)))
                }
            };
            node.unwrap_or_else(|| MediaCondition::Unknown(serialize(std::slice::from_ref(value))))
        }
        ComponentValue::Function(..) => {
            MediaCondition::Unknown(serialize(std::slice::from_ref(value)))
        }
        _ => return None,
    };
    cursor.at += 1;
    cursor.skip_whitespace();
    Some(node)
}

/// Blink's `MediaQueryParser::ConsumeFeature`, over a parenthesized
/// block's contents. On failure, a note on why, when it is not plain
/// nesting or garbage.
fn consume_feature(cursor: &mut Cursor) -> Result<MediaFeature, Option<String>> {
    let start = cursor.at;
    let block_text = format!("({})", serialize(&cursor.values[start..]).trim());
    let mut note = None;

    // <mf-boolean> and <mf-plain>.
    if let Some(raw) = cursor.peek().and_then(ComponentValue::ident) {
        let lower = raw.to_ascii_lowercase();
        cursor.at += 1;
        cursor.skip_whitespace();
        // `-webkit-min-device-pixel-ratio` keeps its vendor prefix first.
        let (prefix, base) = match lower.as_str() {
            "-webkit-min-device-pixel-ratio" => {
                (Some(RangePrefix::Min), "-webkit-device-pixel-ratio")
            }
            "-webkit-max-device-pixel-ratio" => {
                (Some(RangePrefix::Max), "-webkit-device-pixel-ratio")
            }
            _ => match lower.strip_prefix("min-") {
                Some(base) if !base.starts_with('-') => (Some(RangePrefix::Min), base),
                _ => match lower.strip_prefix("max-") {
                    Some(base) if !base.starts_with('-') => (Some(RangePrefix::Max), base),
                    _ => (None, lower.as_str()),
                },
            },
        };
        let named =
            MediaFeatureName::from_name(base).filter(|name| prefix.is_none() || name.is_range());
        match named {
            Some(name) => {
                if cursor.at_end() && prefix.is_none() {
                    return Ok(MediaFeature {
                        name,
                        prefix,
                        test: FeatureTest::Boolean,
                    });
                }
                if matches!(cursor.peek(), Some(ComponentValue::Token(Token::Colon))) {
                    cursor.at += 1;
                    cursor.skip_whitespace();
                    match consume_value(name, cursor) {
                        Ok(value) if cursor.at_end() => {
                            return Ok(MediaFeature {
                                name,
                                prefix,
                                test: FeatureTest::Plain(value),
                            })
                        }
                        Ok(_) => {}
                        Err(Some(reason)) => {
                            note = Some(format!("`{block_text}`: {reason}"));
                        }
                        Err(None) => {}
                    }
                }
            }
            None => {
                if UNSUPPORTED_FEATURES.contains(&base) {
                    note = Some(format!(
                        "`{block_text}`: the `{base}` media feature is not supported yet"
                    ));
                } else if raw.starts_with("--") {
                    note = Some(format!(
                        "`{block_text}`: custom media queries are not supported"
                    ));
                }
            }
        }
        cursor.at = start;
    }

    // <mf-name> <mf-comparison> <mf-value>
    if let Some(name) = range_feature_name(cursor) {
        cursor.skip_whitespace();
        if let Some(op) = consume_comparison(cursor) {
            match consume_value(name, cursor) {
                Ok(value) if cursor.at_end() => {
                    return Ok(MediaFeature {
                        name,
                        prefix: None,
                        test: FeatureTest::Range {
                            left: None,
                            right: Some((op, value)),
                        },
                    })
                }
                Err(Some(reason)) => note = Some(format!("`{block_text}`: {reason}")),
                _ => {}
            }
        }
    }
    cursor.at = start;

    // <mf-value> <mf-comparison> <mf-name> [<mf-comparison> <mf-value>]
    let Some(op_at) = (start..cursor.values.len()).find(|&index| {
        matches!(
            &cursor.values[index],
            ComponentValue::Token(Token::Delim('<' | '>' | '='))
        )
    }) else {
        return Err(note);
    };
    // Blink skips to the first comparison or colon.
    if cursor.values[start..op_at]
        .iter()
        .any(|value| matches!(value, ComponentValue::Token(Token::Colon)))
    {
        return Err(note);
    }
    cursor.at = op_at;
    let Some(op1) = consume_comparison(cursor) else {
        return Err(note);
    };
    let Some(name) = range_feature_name(cursor) else {
        return Err(note);
    };
    cursor.skip_whitespace();
    let after_name = cursor.at;
    cursor.at = start;
    let value1 = match consume_value(name, cursor) {
        Ok(value) => value,
        Err(reason) => return Err(reason.map(|r| format!("`{block_text}`: {r}")).or(note)),
    };
    if cursor.at != op_at {
        return Err(note);
    }
    cursor.at = after_name;
    if cursor.at_end() {
        return Ok(MediaFeature {
            name,
            prefix: None,
            test: FeatureTest::Range {
                left: Some((value1, op1)),
                right: None,
            },
        });
    }
    let Some(op2) = consume_comparison(cursor) else {
        return Err(note);
    };
    if !((op1.is_less() && op2.is_less()) || (op1.is_greater() && op2.is_greater())) {
        return Err(note);
    }
    let value2 = match consume_value(name, cursor) {
        Ok(value) => value,
        Err(reason) => return Err(reason.map(|r| format!("`{block_text}`: {r}")).or(note)),
    };
    // Blink does not require the end here; the caller's block check does.
    Ok(MediaFeature {
        name,
        prefix: None,
        test: FeatureTest::Range {
            left: Some((value1, op1)),
            right: Some((op2, value2)),
        },
    })
}

fn range_feature_name(cursor: &mut Cursor) -> Option<MediaFeatureName> {
    let ident = cursor.peek().and_then(ComponentValue::ident)?;
    let name = MediaFeatureName::from_name(&ident.to_ascii_lowercase())?;
    if !name.is_range() {
        return None;
    }
    cursor.at += 1;
    Some(name)
}

/// Blink's `ConsumeComparison`, with the whitespace after it.
fn consume_comparison(cursor: &mut Cursor) -> Option<Comparison> {
    let op = if cursor.peek_delim('=') {
        cursor.at += 1;
        Comparison::Eq
    } else if cursor.peek_delim('<') {
        cursor.at += 1;
        if cursor.peek_delim('=') {
            cursor.at += 1;
            Comparison::Le
        } else {
            Comparison::Lt
        }
    } else if cursor.peek_delim('>') {
        cursor.at += 1;
        if cursor.peek_delim('=') {
            cursor.at += 1;
            Comparison::Ge
        } else {
            Comparison::Gt
        }
    } else {
        return None;
    };
    cursor.skip_whitespace();
    Some(op)
}

/// Blink's `MediaQueryExpValue::Consume` for `feature`, with the
/// whitespace after the value. `Err(Some(_))` names what Chromium accepts
/// and Div does not evaluate yet.
fn consume_value(
    feature: MediaFeatureName,
    cursor: &mut Cursor,
) -> Result<FeatureValue, Option<String>> {
    use MediaFeatureName as F;
    let value = match cursor.peek() {
        Some(ComponentValue::Token(Token::Number { value, integer })) => FeatureValue::Number {
            value: *value,
            integer: *integer,
        },
        Some(ComponentValue::Token(Token::Dimension { value, unit, .. })) => {
            if let Some(unit) = MediaLengthUnit::from_name(unit) {
                FeatureValue::Length {
                    value: *value,
                    unit,
                }
            } else if let Some(unit) = ResolutionUnit::from_name(unit) {
                FeatureValue::Resolution {
                    value: *value,
                    unit,
                }
            } else if UNSUPPORTED_LENGTH_UNITS.contains(&unit.to_ascii_lowercase().as_str()) {
                return Err(Some(format!(
                    "the `{unit}` unit is not supported in media queries yet"
                )));
            } else {
                return Err(None);
            }
        }
        Some(ComponentValue::Function(name, _)) => {
            let lower = name.to_ascii_lowercase();
            if matches!(
                lower.as_str(),
                "calc" | "-webkit-calc" | "min" | "max" | "clamp"
            ) {
                return Err(Some(format!(
                    "`{name}()` in media queries is not supported yet"
                )));
            }
            return Err(None);
        }
        Some(ComponentValue::Token(Token::Ident(ident))) => {
            let keyword = MediaKeyword::for_feature(feature, ident).ok_or(None)?;
            cursor.at += 1;
            cursor.skip_whitespace();
            return Ok(FeatureValue::Keyword(keyword));
        }
        _ => return Err(None),
    };
    cursor.at += 1;
    cursor.skip_whitespace();

    if feature == F::AspectRatio {
        let FeatureValue::Number {
            value: numerator, ..
        } = value
        else {
            return Err(None);
        };
        if numerator < 0.0 {
            return Err(None);
        }
        if !cursor.peek_delim('/') {
            return Ok(FeatureValue::Ratio {
                numerator,
                denominator: 1.0,
            });
        }
        cursor.at += 1;
        cursor.skip_whitespace();
        let Some(ComponentValue::Token(Token::Number {
            value: denominator, ..
        })) = cursor.peek()
        else {
            return Err(None);
        };
        let denominator = *denominator;
        if denominator < 0.0 {
            return Err(None);
        }
        cursor.at += 1;
        cursor.skip_whitespace();
        if numerator == 0.0 && denominator == 0.0 {
            return Ok(FeatureValue::Ratio {
                numerator: 1.0,
                denominator: 0.0,
            });
        }
        return Ok(FeatureValue::Ratio {
            numerator,
            denominator,
        });
    }

    let valid = match (&value, feature) {
        (FeatureValue::Length { .. }, F::Width | F::Height) => true,
        (FeatureValue::Number { value, .. }, F::Width | F::Height) => *value == 0.0,
        (FeatureValue::Resolution { value, .. }, F::Resolution) => *value >= 0.0,
        (FeatureValue::Number { .. }, F::WebkitDevicePixelRatio) => true,
        _ => false,
    };
    if valid {
        Ok(value)
    } else {
        Err(None)
    }
}

/// Component values back to text, for diagnostics and `<general-enclosed>`.
fn serialize(values: &[ComponentValue]) -> String {
    let mut out = String::new();
    for value in values {
        match value {
            ComponentValue::Token(token) => out.push_str(&token_text(token)),
            ComponentValue::Block(open, inner) => {
                let close = match open {
                    '(' => ')',
                    '[' => ']',
                    _ => '}',
                };
                out.push(*open);
                out.push_str(&serialize(inner));
                out.push(close);
            }
            ComponentValue::Function(name, inner) => {
                out.push_str(name);
                out.push('(');
                out.push_str(&serialize(inner));
                out.push(')');
            }
        }
    }
    out
}

fn number_text(value: f64) -> String {
    format!("{value}")
}

fn token_text(token: &Token) -> String {
    match token {
        Token::Ident(name) => name.clone(),
        Token::Function(name) => format!("{name}("),
        Token::AtKeyword(name) => format!("@{name}"),
        Token::Hash(name) => format!("#{name}"),
        Token::String(text) => format!("\"{text}\""),
        Token::BadString => "\"".to_owned(),
        Token::Url(text) => format!("url({text})"),
        Token::BadUrl => "url(".to_owned(),
        Token::Delim(c) => c.to_string(),
        Token::Number { value, .. } => number_text(*value),
        Token::Percentage(value) => format!("{}%", number_text(*value)),
        Token::Dimension { value, unit, .. } => format!("{}{unit}", number_text(*value)),
        Token::Whitespace => " ".to_owned(),
        Token::Cdo => "<!--".to_owned(),
        Token::Cdc => "-->".to_owned(),
        Token::Colon => ":".to_owned(),
        Token::Semicolon => ";".to_owned(),
        Token::Comma => ",".to_owned(),
        Token::OpenParen => "(".to_owned(),
        Token::CloseParen => ")".to_owned(),
        Token::OpenBracket => "[".to_owned(),
        Token::CloseBracket => "]".to_owned(),
        Token::OpenBrace => "{".to_owned(),
        Token::CloseBrace => "}".to_owned(),
    }
}

// ── Serialization ───────────────────────────────────────────────────────

/// CSSOM's "serialize a media query list", as Chromium 147's
/// `MediaQuerySet::MediaText` produces it: queries joined by `", "`, each
/// as Blink's `MediaQuery::Serialize` and `MediaQueryExp::Serialize`
/// (`core/css/media_query.cc`, `media_query_exp.cc`, BSD) write it. This is
/// `matchMedia(query).media`.
impl std::fmt::Display for MediaQueryListData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, query) in self.queries.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{query}")?;
        }
        Ok(())
    }
}

/// Blink's `MediaQuery::Serialize`: the restrictor, then the media type
/// unless it is a plain `all` with a condition, then the condition.
impl std::fmt::Display for MediaQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.restrictor {
            MediaRestrictor::Only => f.write_str("only ")?,
            MediaRestrictor::Not => f.write_str("not ")?,
            MediaRestrictor::None => {}
        }
        let media_type = match &self.media_type {
            MediaType::All => "all",
            MediaType::Screen => "screen",
            MediaType::Print => "print",
            MediaType::Other(name) => name,
        };
        let Some(condition) = &self.condition else {
            return f.write_str(media_type);
        };
        if self.media_type != MediaType::All || self.restrictor != MediaRestrictor::None {
            write!(f, "{media_type} and ")?;
        }
        write_condition(f, condition)
    }
}

/// A condition. A parenthesized condition is a `Nested` node and a
/// feature brings its own parentheses, so the text keeps the source's
/// grouping, as Blink's `ConditionalExpNode` serializers do; a
/// `<general-enclosed>` term keeps its own text.
fn write_condition(
    f: &mut std::fmt::Formatter<'_>,
    condition: &MediaCondition,
) -> std::fmt::Result {
    match condition {
        MediaCondition::Feature(feature) => write!(f, "({feature})"),
        MediaCondition::Unknown(text) => f.write_str(text),
        MediaCondition::Nested(inner) => {
            f.write_str("(")?;
            write_condition(f, inner)?;
            f.write_str(")")
        }
        MediaCondition::Not(inner) => {
            f.write_str("not ")?;
            write_condition(f, inner)
        }
        MediaCondition::And(terms) | MediaCondition::Or(terms) => {
            let joiner = if matches!(condition, MediaCondition::And(_)) {
                " and "
            } else {
                " or "
            };
            for (index, term) in terms.iter().enumerate() {
                if index > 0 {
                    f.write_str(joiner)?;
                }
                write_condition(f, term)?;
            }
            Ok(())
        }
    }
}

impl MediaFeatureName {
    fn css_name(self) -> &'static str {
        match self {
            Self::Width => "width",
            Self::Height => "height",
            Self::AspectRatio => "aspect-ratio",
            Self::Orientation => "orientation",
            Self::Resolution => "resolution",
            Self::WebkitDevicePixelRatio => "-webkit-device-pixel-ratio",
            Self::PrefersColorScheme => "prefers-color-scheme",
            Self::PrefersReducedMotion => "prefers-reduced-motion",
            Self::Hover => "hover",
            Self::AnyHover => "any-hover",
            Self::Pointer => "pointer",
            Self::AnyPointer => "any-pointer",
            Self::ForcedColors => "forced-colors",
        }
    }
}

/// Blink's `MediaQueryExp::Serialize`, without the parentheses.
impl std::fmt::Display for MediaFeature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let base = self.name.css_name();
        let name = match self.prefix {
            None => base.to_owned(),
            Some(prefix) => {
                let prefix = match prefix {
                    RangePrefix::Min => "min-",
                    RangePrefix::Max => "max-",
                };
                match base.strip_prefix("-webkit-") {
                    Some(rest) => format!("-webkit-{prefix}{rest}"),
                    None => format!("{prefix}{base}"),
                }
            }
        };
        match &self.test {
            FeatureTest::Boolean => f.write_str(&name),
            FeatureTest::Plain(value) => write!(f, "{name}: {value}"),
            FeatureTest::Range { left, right } => {
                if let Some((value, op)) = left {
                    write!(f, "{value} {} ", op.css_text())?;
                }
                f.write_str(&name)?;
                if let Some((op, value)) = right {
                    write!(f, " {} {value}", op.css_text())?;
                }
                Ok(())
            }
        }
    }
}

impl Comparison {
    fn css_text(self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
        }
    }
}

impl std::fmt::Display for FeatureValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Length { value, unit } => {
                let unit = match unit {
                    MediaLengthUnit::Px => "px",
                    MediaLengthUnit::Cm => "cm",
                    MediaLengthUnit::Mm => "mm",
                    MediaLengthUnit::Q => "q",
                    MediaLengthUnit::In => "in",
                    MediaLengthUnit::Pt => "pt",
                    MediaLengthUnit::Pc => "pc",
                    MediaLengthUnit::Em => "em",
                    MediaLengthUnit::Rem => "rem",
                    MediaLengthUnit::Vw => "vw",
                    MediaLengthUnit::Vh => "vh",
                    MediaLengthUnit::Vmin => "vmin",
                    MediaLengthUnit::Vmax => "vmax",
                };
                write!(f, "{}{unit}", css_number(*value))
            }
            Self::Number { value, .. } => f.write_str(&css_number(*value)),
            Self::Ratio {
                numerator,
                denominator,
            } => write!(
                f,
                "{} / {}",
                css_number(*numerator),
                css_number(*denominator)
            ),
            Self::Resolution { value, unit } => {
                let unit = match unit {
                    ResolutionUnit::Dppx => "dppx",
                    ResolutionUnit::X => "x",
                    ResolutionUnit::Dpi => "dpi",
                    ResolutionUnit::Dpcm => "dpcm",
                };
                write!(f, "{}{unit}", css_number(*value))
            }
            Self::Keyword(keyword) => f.write_str(match keyword {
                MediaKeyword::Portrait => "portrait",
                MediaKeyword::Landscape => "landscape",
                MediaKeyword::None => "none",
                MediaKeyword::Hover => "hover",
                MediaKeyword::Coarse => "coarse",
                MediaKeyword::Fine => "fine",
                MediaKeyword::Light => "light",
                MediaKeyword::Dark => "dark",
                MediaKeyword::NoPreference => "no-preference",
                MediaKeyword::Reduce => "reduce",
                MediaKeyword::Active => "active",
            }),
        }
    }
}

/// A number as Chromium serializes a CSS numeric value: like C's `%g`,
/// six significant digits without trailing zeros, in exponent form below
/// 1e-4 and from 1e6 on (`1.23457e+07`, `1e-06`). Negative zero is `0`.
pub(crate) fn css_number(value: f64) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    if !value.is_finite() {
        return if value.is_nan() {
            "NaN".to_owned()
        } else if value > 0.0 {
            "inf".to_owned()
        } else {
            "-inf".to_owned()
        };
    }
    // Round to six significant digits first; the exponent of the rounded
    // value picks the form, as `%g` does.
    let scientific = format!("{value:.5e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("`{:e}` output has an exponent");
    let exponent: i32 = exponent.parse().expect("`{:e}` exponent is an integer");
    if !(-4..6).contains(&exponent) {
        let mantissa = trim_fraction(mantissa);
        let sign = if exponent < 0 { '-' } else { '+' };
        return format!("{mantissa}e{sign}{:02}", exponent.abs());
    }
    let decimals = (5 - exponent) as usize;
    trim_fraction(&format!("{value:.decimals$}")).to_owned()
}

fn trim_fraction(text: &str) -> &str {
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        text
    }
}

// ── Evaluation ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kleene {
    True,
    False,
    Unknown,
}

impl From<bool> for Kleene {
    fn from(value: bool) -> Self {
        if value {
            Self::True
        } else {
            Self::False
        }
    }
}

impl MediaQueryListData {
    /// Whether the list matches: an empty list does, otherwise any query.
    pub fn matches(&self, values: &MediaValues) -> bool {
        self.queries.is_empty() || self.queries.iter().any(|query| query.matches(values))
    }
}

impl MediaQuery {
    pub(crate) fn matches(&self, values: &MediaValues) -> bool {
        let type_matches = matches!(self.media_type, MediaType::All | MediaType::Screen);
        let value = if !type_matches {
            Kleene::False
        } else {
            match &self.condition {
                None => Kleene::True,
                Some(condition) => condition.evaluate(values),
            }
        };
        // Blink's `ApplyRestrictor`: unknown never matches, even negated.
        match (value, self.restrictor) {
            (Kleene::Unknown, _) => false,
            (value, MediaRestrictor::Not) => value == Kleene::False,
            (value, _) => value == Kleene::True,
        }
    }
}

impl MediaCondition {
    fn evaluate(&self, values: &MediaValues) -> Kleene {
        match self {
            Self::Feature(feature) => feature.evaluate(values).into(),
            Self::Unknown(_) => Kleene::Unknown,
            Self::Nested(inner) => inner.evaluate(values),
            Self::Not(inner) => match inner.evaluate(values) {
                Kleene::True => Kleene::False,
                Kleene::False => Kleene::True,
                Kleene::Unknown => Kleene::Unknown,
            },
            Self::And(terms) => {
                let mut result = Kleene::True;
                for term in terms {
                    match term.evaluate(values) {
                        Kleene::False => return Kleene::False,
                        Kleene::Unknown => result = Kleene::Unknown,
                        Kleene::True => {}
                    }
                }
                result
            }
            Self::Or(terms) => {
                let mut result = Kleene::False;
                for term in terms {
                    match term.evaluate(values) {
                        Kleene::True => return Kleene::True,
                        Kleene::Unknown => result = Kleene::Unknown,
                        Kleene::False => {}
                    }
                }
                result
            }
        }
    }
}

/// Blink's comparison operator, where `None` is a plain test.
#[derive(Clone, Copy)]
enum Operator {
    None,
    Compare(Comparison),
}

impl MediaFeature {
    /// Blink's `EvalFeature`: the right bound as written, the left bound
    /// with its operator reversed.
    fn evaluate(&self, values: &MediaValues) -> bool {
        match &self.test {
            FeatureTest::Boolean => evaluate_feature(self.name, None, Operator::None, values),
            FeatureTest::Plain(value) => {
                let op = match self.prefix {
                    Some(RangePrefix::Min) => Operator::Compare(Comparison::Ge),
                    Some(RangePrefix::Max) => Operator::Compare(Comparison::Le),
                    None => Operator::None,
                };
                evaluate_feature(self.name, Some(value), op, values)
            }
            FeatureTest::Range { left, right } => {
                let mut result = true;
                if let Some((op, value)) = right {
                    result &=
                        evaluate_feature(self.name, Some(value), Operator::Compare(*op), values);
                }
                if let Some((value, op)) = left {
                    result &= evaluate_feature(
                        self.name,
                        Some(value),
                        Operator::Compare(op.reversed()),
                        values,
                    );
                }
                result
            }
        }
    }
}

/// Blink's `HandleNegativeMediaFeatureValue`.
fn negative_query(op: Operator) -> bool {
    matches!(op, Operator::Compare(Comparison::Gt | Comparison::Ge))
}

/// Blink's `CompareValue<float>`.
fn compare_float(actual: f32, query: f32, op: Operator) -> bool {
    if query < 0.0 {
        return negative_query(op);
    }
    match op {
        Operator::Compare(Comparison::Ge) => actual >= query,
        Operator::Compare(Comparison::Le) => actual <= query,
        Operator::None | Operator::Compare(Comparison::Eq) => actual == query,
        Operator::Compare(Comparison::Lt) => actual < query,
        Operator::Compare(Comparison::Gt) => actual > query,
    }
}

/// Blink's `CompareDoubleValue`, which allows one layout unit (1/64px) of
/// slack for `>=`, `<=` and `=`.
fn compare_double(actual: f64, query: f64, op: Operator) -> bool {
    if query < 0.0 {
        return negative_query(op);
    }
    const PRECISION: f64 = 1.0 / 64.0;
    match op {
        Operator::Compare(Comparison::Ge) => actual >= query - PRECISION,
        Operator::Compare(Comparison::Le) => actual <= query + PRECISION,
        Operator::None | Operator::Compare(Comparison::Eq) => (actual - query).abs() <= PRECISION,
        Operator::Compare(Comparison::Lt) => actual < query,
        Operator::Compare(Comparison::Gt) => actual > query,
    }
}

/// A media query length in CSS pixels (Blink's `ZoomedComputedPixels` at
/// zoom 1, with the initial font for `em` and `rem`).
fn length_pixels(value: f64, unit: MediaLengthUnit, values: &MediaValues) -> f64 {
    use MediaLengthUnit as U;
    match unit {
        U::Px => value,
        U::Cm => value * (96.0 / 2.54),
        U::Mm => value * (96.0 / 25.4),
        U::Q => value * (96.0 / 101.6),
        U::In => value * 96.0,
        U::Pt => value * (96.0 / 72.0),
        U::Pc => value * (96.0 / 6.0),
        U::Em | U::Rem => value * values.initial_font_size,
        U::Vw => value * values.width / 100.0,
        U::Vh => value * values.height / 100.0,
        U::Vmin => value * values.width.min(values.height) / 100.0,
        U::Vmax => value * values.width.max(values.height) / 100.0,
    }
}

fn evaluate_feature(
    name: MediaFeatureName,
    value: Option<&FeatureValue>,
    op: Operator,
    values: &MediaValues,
) -> bool {
    use MediaFeatureName as F;
    match name {
        F::Width | F::Height => {
            let actual = if name == F::Width {
                values.width
            } else {
                values.height
            };
            match value {
                None => actual != 0.0,
                Some(FeatureValue::Length { value, unit }) => {
                    compare_double(actual, length_pixels(*value, *unit, values), op)
                }
                // Only zero parses (Blink's `ComputeLength` clamps to int).
                Some(FeatureValue::Number { value, .. }) => {
                    compare_double(actual, f64::from(*value as i32), op)
                }
                Some(_) => false,
            }
        }
        F::AspectRatio => match value {
            None => true,
            // Blink's `CompareAspectRatioValue` takes the viewport size as
            // integers.
            Some(FeatureValue::Ratio {
                numerator,
                denominator,
            }) => compare_double(
                f64::from(values.width as i32) * denominator,
                f64::from(values.height as i32) * numerator,
                op,
            ),
            Some(_) => false,
        },
        F::Orientation => match value {
            None => values.width >= 0.0 && values.height >= 0.0,
            Some(FeatureValue::Keyword(keyword)) => {
                if values.width > values.height {
                    *keyword == MediaKeyword::Landscape
                } else {
                    *keyword == MediaKeyword::Portrait
                }
            }
            Some(_) => false,
        },
        F::Resolution | F::WebkitDevicePixelRatio => {
            let actual = values.device_pixel_ratio as f32;
            match value {
                None => actual != 0.0,
                Some(FeatureValue::Number { value, .. }) if name == F::WebkitDevicePixelRatio => {
                    compare_float(actual, *value as f32, op)
                }
                Some(FeatureValue::Resolution { value, unit }) if name == F::Resolution => {
                    let dppx = match unit {
                        ResolutionUnit::Dppx | ResolutionUnit::X => *value,
                        ResolutionUnit::Dpi => *value / 96.0,
                        ResolutionUnit::Dpcm => *value * 2.54 / 96.0,
                    } as f32;
                    if *unit == ResolutionUnit::Dpcm {
                        // Blink compares dpcm at two decimals.
                        let round = |v: f32| (0.5 + 100.0 * v).floor() / 100.0;
                        compare_float(round(actual), round(dppx), op)
                    } else {
                        compare_float(actual, dppx, op)
                    }
                }
                Some(_) => false,
            }
        }
        F::PrefersColorScheme => match value {
            None => true,
            Some(FeatureValue::Keyword(MediaKeyword::Dark)) => {
                values.color_scheme == ColorScheme::Dark
            }
            Some(FeatureValue::Keyword(MediaKeyword::Light)) => {
                values.color_scheme == ColorScheme::Light
            }
            Some(_) => false,
        },
        F::PrefersReducedMotion => match value {
            None => values.reduced_motion,
            Some(FeatureValue::Keyword(MediaKeyword::NoPreference)) => !values.reduced_motion,
            Some(FeatureValue::Keyword(MediaKeyword::Reduce)) => values.reduced_motion,
            Some(_) => false,
        },
        F::Hover => match value {
            None => values.hover != HoverCapability::None,
            Some(FeatureValue::Keyword(MediaKeyword::None)) => {
                values.hover == HoverCapability::None
            }
            Some(FeatureValue::Keyword(MediaKeyword::Hover)) => {
                values.hover == HoverCapability::Hover
            }
            Some(_) => false,
        },
        F::AnyHover => match value {
            None => values.any_hover,
            Some(FeatureValue::Keyword(MediaKeyword::None)) => values.any_hover_none,
            Some(FeatureValue::Keyword(MediaKeyword::Hover)) => values.any_hover,
            Some(_) => false,
        },
        F::Pointer => match value {
            None => values.pointer != PointerAccuracy::None,
            Some(FeatureValue::Keyword(MediaKeyword::None)) => {
                values.pointer == PointerAccuracy::None
            }
            Some(FeatureValue::Keyword(MediaKeyword::Coarse)) => {
                values.pointer == PointerAccuracy::Coarse
            }
            Some(FeatureValue::Keyword(MediaKeyword::Fine)) => {
                values.pointer == PointerAccuracy::Fine
            }
            Some(_) => false,
        },
        F::AnyPointer => match value {
            None => values.any_pointer_coarse || values.any_pointer_fine,
            Some(FeatureValue::Keyword(MediaKeyword::None)) => values.any_pointer_none,
            Some(FeatureValue::Keyword(MediaKeyword::Coarse)) => values.any_pointer_coarse,
            Some(FeatureValue::Keyword(MediaKeyword::Fine)) => values.any_pointer_fine,
            Some(_) => false,
        },
        // Div has no forced colors mode.
        F::ForcedColors => matches!(value, Some(FeatureValue::Keyword(MediaKeyword::None))),
    }
}

#[cfg(test)]
#[path = "media_query_tests.rs"]
mod tests;
