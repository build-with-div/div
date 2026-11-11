use std::ffi::c_void;

pub const DIVUI_ABI_VERSION: u32 = 0x0002_0000;

#[repr(C)]
pub struct DivUiApp {
    _private: [u8; 0],
}

#[repr(C)]
pub struct DivUiDocument {
    _private: [u8; 0],
}

#[repr(C)]
pub struct DivUiElement {
    _private: [u8; 0],
}

#[repr(C)]
pub struct DivUiStyleCompound {
    _private: [u8; 0],
}

#[repr(C)]
pub struct DivUiResource {
    _private: [u8; 0],
}

#[repr(C)]
pub struct DivUiFontFace {
    _private: [u8; 0],
}

#[repr(C)]
pub struct DivUiListener {
    _private: [u8; 0],
}

#[repr(C)]
pub struct DivUiBuffer {
    _private: [u8; 0],
}

#[repr(C)]
pub struct DivUiAccessibilitySnapshot {
    _private: [u8; 0],
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivUiStatus {
    Ok = 0,
    InvalidArgument = -1,
    AbiMismatch = -2,
    WrongThread = -3,
    InvalidHandle = -4,
    WrongDocument = -5,
    StaleHandle = -6,
    WrongValueType = -7,
    OutOfMemory = -8,
    InvalidState = -9,
    BufferTooSmall = -10,
    Reentrant = -11,
    Internal = -99,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiUtf8 {
    pub data: *const u8,
    pub length: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiLength {
    pub value: f32,
    pub unit: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union DivUiStylePayload {
    pub length: DivUiLength,
    pub number: f32,
    pub integer: i32,
    pub color: DivUiColor,
    pub enum_value: i32,
    pub compound: *const DivUiStyleCompound,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiStyleValue {
    pub tag: u32,
    pub reserved: u32,
    pub data: DivUiStylePayload,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiAppConfig {
    pub struct_size: u32,
    pub abi_version: u32,
    pub title: DivUiUtf8,
    pub width: u32,
    pub height: u32,
    pub backend: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiViewportMetrics {
    pub logical_width: f64,
    pub logical_height: f64,
    pub physical_width: u32,
    pub physical_height: u32,
    pub device_scale_factor: f64,
    pub authority: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiDocumentConfig {
    pub struct_size: u32,
    pub abi_version: u32,
    pub viewport: DivUiViewportMetrics,
}

/// Caller-owned client and content dimensions in logical CSS pixels.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DivUiScrollMetricsV1 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub client_width: f64,
    pub client_height: f64,
    pub scroll_width: f64,
    pub scroll_height: f64,
}

/// Owned numerical scroll-button state in logical CSS pixels. Related node
/// identities are queried separately as independently owned element aliases.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DivUiScrollButtonStateV1 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub direction: u32,
    pub availability: u32,
    pub enabled: u8,
    pub has_target: u8,
    pub reserved: [u8; 6],
    pub current_x: f64,
    pub current_y: f64,
    pub target_x: f64,
    pub target_y: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiFontUnicodeRange {
    pub start: u32,
    pub end: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiFontFeatureDefault {
    pub tag: [u8; 4],
    pub value: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiFontFaceDescriptor {
    pub struct_size: u32,
    pub abi_version: u32,
    pub family: DivUiUtf8,
    pub face_index: u32,
    pub style: u32,
    pub style_min: f32,
    pub style_max: f32,
    pub weight_min: f32,
    pub weight_max: f32,
    pub stretch_min: f32,
    pub stretch_max: f32,
    pub unicode_ranges: *const DivUiFontUnicodeRange,
    pub unicode_range_count: usize,
    pub feature_defaults: *const DivUiFontFeatureDefault,
    pub feature_default_count: usize,
    pub size_adjust: f32,
    pub ascent_override: f32,
    pub descent_override: f32,
    pub line_gap_override: f32,
    pub flags: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiFontFaceInfo {
    pub struct_size: u32,
    pub abi_version: u32,
    pub collection_id: u64,
    pub face_id: u64,
    pub collection_generation: u64,
    pub byte_length: usize,
    pub format: u32,
    pub face_index: u32,
    pub style: u32,
    pub flags: u32,
    pub style_min: f32,
    pub style_max: f32,
    pub weight_min: f32,
    pub weight_max: f32,
    pub stretch_min: f32,
    pub stretch_max: f32,
    pub size_adjust: f32,
    pub ascent_override: f32,
    pub descent_override: f32,
    pub line_gap_override: f32,
    pub family_length: usize,
    pub unicode_range_count: usize,
    pub feature_default_count: usize,
    pub sha256: [u8; 32],
}

pub const DIVUI_FONT_FACE_STYLE_NORMAL: u32 = 0;
pub const DIVUI_FONT_FACE_STYLE_ITALIC: u32 = 1;
pub const DIVUI_FONT_FACE_STYLE_OBLIQUE: u32 = 2;
pub const DIVUI_FONT_CONTAINER_TTF: u32 = 1;
pub const DIVUI_FONT_CONTAINER_OTF: u32 = 2;
pub const DIVUI_FONT_CONTAINER_COLLECTION: u32 = 3;
pub const DIVUI_FONT_CONTAINER_WOFF: u32 = 4;
pub const DIVUI_FONT_CONTAINER_WOFF2: u32 = 5;
pub const DIVUI_FONT_FACE_HAS_SIZE_ADJUST: u32 = 1 << 0;
pub const DIVUI_FONT_FACE_HAS_ASCENT_OVERRIDE: u32 = 1 << 1;
pub const DIVUI_FONT_FACE_HAS_DESCENT_OVERRIDE: u32 = 1 << 2;
pub const DIVUI_FONT_FACE_HAS_LINE_GAP_OVERRIDE: u32 = 1 << 3;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DivUiRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiBitmap {
    pub struct_size: u32,
    pub abi_version: u32,
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub pixels: *mut DivUiBuffer,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiTransformOperation {
    pub kind: u32,
    pub reserved: u32,
    pub x: DivUiLength,
    pub y: DivUiLength,
    pub values: [f32; 6],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiEasing {
    pub kind: u32,
    pub step_position: u32,
    pub step_count: u32,
    pub reserved: u32,
    pub values: [f64; 4],
}

/// A caller-owned normalized or authored-percentage linear stop.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiLinearStopV1 {
    pub input: f64,
    pub output: f64,
}

/// The known prefix of a caller-owned curve; future tails are ignored.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiEasingCurveV1 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub kind: u32,
    pub step_position: u32,
    pub step_count: u32,
    pub reserved: u32,
    pub values: [f64; 4],
    pub stops: *const DivUiLinearStopV1,
    pub stop_count: usize,
    pub stop_stride: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiAnimationOptionsV1 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub delay_ms: f64,
    pub duration_ms: f64,
    pub iterations: f64,
    pub playback_rate: f64,
    pub direction: u32,
    pub fill: u32,
    pub play_state: u32,
    pub composite: u32,
    pub easing: *const DivUiEasingCurveV1,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiKeyframeV1 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub offset: f64,
    pub value: DivUiStyleValue,
    pub easing: *const DivUiEasingCurveV1,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiAnimationOptions {
    pub struct_size: u32,
    pub abi_version: u32,
    pub delay_ms: f64,
    pub duration_ms: f64,
    pub iterations: f64,
    pub playback_rate: f64,
    pub direction: u32,
    pub fill: u32,
    pub play_state: u32,
    pub composite: u32,
    pub easing: DivUiEasing,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiAnimationTimeline {
    pub struct_size: u32,
    pub abi_version: u32,
    pub kind: u32,
    pub axis: u32,
    pub source: *mut DivUiElement,
    pub range_start: f64,
    pub range_end: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiKeyframe {
    pub offset: f64,
    pub value: DivUiStyleValue,
    pub easing: DivUiEasing,
    pub has_easing: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiAnimationState {
    pub struct_size: u32,
    pub abi_version: u32,
    pub animation_id: u64,
    pub current_time_ms: f64,
    pub iteration: u64,
    pub property: i32,
    pub play_state: u32,
    pub phase: u32,
    pub finished: u8,
    pub reserved: [u8; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiAnimationEvent {
    pub struct_size: u32,
    pub abi_version: u32,
    pub animation_id: u64,
    pub kind: u32,
    pub property: i32,
    pub elapsed_time_ms: f64,
    pub iteration: u64,
    pub target: *mut DivUiElement,
}

pub const DIVUI_EVENT_FLAG_DEFAULT_PREVENTED: u32 = 1 << 0;
pub const DIVUI_EVENT_FLAG_PROPAGATION_STOPPED: u32 = 1 << 1;
pub const DIVUI_EVENT_FLAG_IMMEDIATE_PROPAGATION_STOPPED: u32 = 1 << 2;
pub const DIVUI_EDIT_MOVE: u32 = 0;
pub const DIVUI_EDIT_DELETE: u32 = 1;
pub const DIVUI_EDIT_SELECT_ALL: u32 = 2;
pub const DIVUI_EDIT_UNDO: u32 = 3;
pub const DIVUI_EDIT_REDO: u32 = 4;
pub const DIVUI_TEXT_BACKWARD: u32 = 0;
pub const DIVUI_TEXT_FORWARD: u32 = 1;
pub const DIVUI_TEXT_GRAPHEME: u32 = 0;
pub const DIVUI_TEXT_WORD: u32 = 1;
pub const DIVUI_TEXT_LINE: u32 = 2;
pub const DIVUI_TEXT_DOCUMENT: u32 = 3;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiEditCommandV1 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub command: u32,
    pub direction: u32,
    pub unit: u32,
    pub extend_selection: u32,
    pub reserved: [u32; 2],
}

pub const DIVUI_CONTROL_DISABLED: u32 = 1 << 0;
pub const DIVUI_CONTROL_CHECKED: u32 = 1 << 1;
pub const DIVUI_CONTROL_SELECTED: u32 = 1 << 2;
pub const DIVUI_CONTROL_OPEN: u32 = 1 << 3;
pub const DIVUI_CONTROL_INDETERMINATE: u32 = 1 << 4;
pub const DIVUI_CONTROL_PASSWORD: u32 = 1 << 5;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiAccessibilityUpdate {
    pub struct_size: u32,
    pub abi_version: u32,
    pub generation: u64,
    pub updated_nodes: usize,
    pub focus_id: u64,
    pub full_tree: u8,
    pub reduced_motion: u8,
    pub reserved: u16,
}

pub const DIVUI_ACCESSIBILITY_NODE_HIDDEN: u32 = 1 << 0;
pub const DIVUI_ACCESSIBILITY_NODE_REQUIRED: u32 = 1 << 1;
pub const DIVUI_ACCESSIBILITY_NODE_READ_ONLY: u32 = 1 << 2;
pub const DIVUI_ACCESSIBILITY_NODE_MODAL: u32 = 1 << 3;
pub const DIVUI_ACCESSIBILITY_NODE_HAS_BOUNDS: u32 = 1 << 4;

pub const DIVUI_ACCESSIBILITY_SCROLL_HAS_X: u32 = 1 << 0;
pub const DIVUI_ACCESSIBILITY_SCROLL_HAS_Y: u32 = 1 << 1;
pub const DIVUI_ACCESSIBILITY_SCROLL_HAS_X_MIN: u32 = 1 << 2;
pub const DIVUI_ACCESSIBILITY_SCROLL_HAS_X_MAX: u32 = 1 << 3;
pub const DIVUI_ACCESSIBILITY_SCROLL_HAS_Y_MIN: u32 = 1 << 4;
pub const DIVUI_ACCESSIBILITY_SCROLL_HAS_Y_MAX: u32 = 1 << 5;
pub const DIVUI_ACCESSIBILITY_SCROLL_DIRECTION_ACTION_MASK: u32 =
    (1 << 11) | (1 << 12) | (1 << 13) | (1 << 14);

/// Owned accessibility scroll metadata in native logical coordinates. Absent
/// optional fields are zero; `flags` preserves their presence independently.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DivUiAccessibilityScrollInfoV1 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub flags: u32,
    pub actions: u32,
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub scroll_x_min: f64,
    pub scroll_x_max: f64,
    pub scroll_y_min: f64,
    pub scroll_y_max: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct DivUiAccessibilitySnapshotInfo {
    pub struct_size: u32,
    pub abi_version: u32,
    pub generation: u64,
    pub focus_id: u64,
    pub node_count: usize,
    pub changed_count: usize,
    pub removed_count: usize,
    pub full_tree: u8,
    pub reduced_motion: u8,
    pub reserved: [u8; 6],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiAccessibilityNodeInfo {
    pub struct_size: u32,
    pub abi_version: u32,
    pub id: u64,
    pub role: u32,
    pub flags: u32,
    pub actions: u32,
    pub reserved: u32,
    pub bounds: DivUiRect,
    pub child_count: usize,
    pub labelled_by_count: usize,
    pub described_by_count: usize,
    pub controls_count: usize,
    pub details_count: usize,
    pub label_length: usize,
    pub description_length: usize,
    pub value_length: usize,
    pub role_name_length: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct DivUiAccessibilityNodeState {
    pub struct_size: u32,
    pub abi_version: u32,
    pub flags: u32,
    pub live: u32,
    pub numeric_value: f64,
    pub numeric_min: f64,
    pub numeric_max: f64,
    pub numeric_step: f64,
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub placeholder_length: usize,
    pub character_lengths_count: usize,
    pub selection_anchor_node: u64,
    pub selection_anchor_index: usize,
    pub selection_focus_node: u64,
    pub selection_focus_index: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiEvent {
    pub struct_size: u32,
    pub abi_version: u32,
    pub event_type: u32,
    pub phase: u32,
    pub flags: u32,
    pub modifiers: u32,
    pub timestamp_ns: u64,
    pub x: f32,
    pub y: f32,
    pub delta_x: f32,
    pub delta_y: f32,
    pub key_code: i32,
    pub pointer_id: u32,
    pub text: DivUiUtf8,
    pub target: *mut DivUiElement,
    pub current_target: *mut DivUiElement,
}

/// Owned metadata copied from a currently executing native focus callback.
/// The related element handle must be released with divui_element_destroy.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiFocusEventInfoV1 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub bubbles: u32,
    pub cancelable: u32,
    pub related_target: *mut DivUiElement,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiErrorInfo {
    pub struct_size: u32,
    pub abi_version: u32,
    pub status: i32,
    pub detail: u32,
    pub message_length: usize,
}

pub type DivUiEventCallback = unsafe extern "C" fn(event: *mut DivUiEvent, user_data: *mut c_void);

/// Borrowed platform event; all text/path storage is valid only during callback.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiPlatformEvent {
    pub struct_size: u32,
    pub abi_version: u32,
    pub event_type: u32,
    pub phase: u32,
    pub flags: u32,
    pub backend: u32,
    pub pointer_id: u64,
    pub x: f32,
    pub y: f32,
    pub delta_x: f32,
    pub delta_y: f32,
    pub key_code: i32,
    pub modifiers: u32,
    pub button: u32,
    pub reserved: u32,
    pub frame_number: u64,
    pub time_ms: f64,
    pub text: DivUiUtf8,
    pub path: *const u8,
    pub path_length: usize,
    pub viewport: DivUiViewportMetrics,
}

pub type DivUiPlatformEventCallback = unsafe extern "C" fn(
    app: *mut DivUiApp,
    event: *const DivUiPlatformEvent,
    user_data: *mut c_void,
);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiAppRunConfig {
    pub struct_size: u32,
    pub abi_version: u32,
    pub flags: u32,
    pub reserved: u32,
    pub callback: Option<DivUiPlatformEventCallback>,
    pub user_data: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiInputEventInfoV1 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub input_type: u32,
    pub has_data: u32,
    pub is_composing: u32,
    pub bubbles: u32,
    pub cancelable: u32,
    pub data: *mut DivUiBuffer,
}

/// Immutable event properties for an active native callback.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DivUiEventPropertiesV1 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub bubbles: u32,
    pub cancelable: u32,
}

/// Versioned timeline descriptors carry f64 inputs checked into native f32 lengths.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiTimelineLengthV1 {
    pub value: f64,
    pub calc_offset: f64,
    pub unit: u32,
    pub reserved: u32,
}
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiTimelineRangeOffsetV1 {
    pub name: u32,
    pub reserved: u32,
    pub offset: DivUiTimelineLengthV1,
}
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiAnimationAttachmentRangeV1 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub present: u32,
    pub reserved: u32,
    pub start: DivUiTimelineRangeOffsetV1,
    pub end: DivUiTimelineRangeOffsetV1,
}
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiAnimationTimelineSourceV1 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub kind: u32,
    pub axis: u32,
    pub source: *mut DivUiElement,
    pub inset_present: u32,
    pub reserved: u32,
    pub inset_start: DivUiTimelineLengthV1,
    pub inset_end: DivUiTimelineLengthV1,
}
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiTimelineRangeBoundsV1 {
    pub start: f64,
    pub end: f64,
}
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiAnimationTimelineSnapshotV1 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub present: u32,
    pub active: u32,
    pub physical_axis: u32,
    pub reserved: u32,
    pub source: *mut DivUiElement,
    pub scroll_offset: f64,
    pub cover_start: f64,
    pub cover_end: f64,
    pub range_start: f64,
    pub range_end: f64,
    pub duration_ms: f64,
    pub current_time_ms: f64,
    pub current_time_percentage: f64,
}
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiAnimationAttachmentSnapshotV1 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub present: u32,
    pub reserved: u32,
    pub range: DivUiAnimationAttachmentRangeV1,
    pub source: *mut DivUiElement,
    pub named: [DivUiTimelineRangeBoundsV1; 7],
    pub attachment: DivUiTimelineRangeBoundsV1,
    pub entry_crossing_distance: f64,
    pub exit_crossing_distance: f64,
}

/// Independent native radius-axis source length. Unit13 is calculated percent
/// plus pixels; all other units retain the existing DivUiLength unit numbers.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DivUiRadiusAxisLengthV1 {
    pub struct_size: u32,
    pub abi_version: u32,
    pub value: f32,
    pub calc_offset: f32,
    pub unit: u32,
    pub reserved: u32,
}
