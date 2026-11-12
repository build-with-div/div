/* Native C11/C++17 consuming app: owned named descriptors and shared controls. */
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include "divui.h"
#ifdef __cplusplus
#define ABI(t, n) static_assert(sizeof(t) == n, #t)
#else
#define ABI(t, n) _Static_assert(sizeof(t) == n, #t)
#endif
ABI(DivUiAnimationTimeline, 40);
ABI(DivUiTimelineLengthV1, 24);
ABI(DivUiAnimationAttachmentRangeV1, 80);
ABI(DivUiAnimationTimelineSourceV1, 80);
ABI(DivUiAnimationTimelineSnapshotV1, 96);
ABI(DivUiAnimationAttachmentSnapshotV1, 248);
static DivUiUtf8 text(const char* s) {
  DivUiUtf8 v = {(const uint8_t*)s, strlen(s)};
  return v;
}
static void style(DivUiElement* n, DivUiStyleProperty p, const char* s) {
  DivUiStyleValue v;
  assert(divui_style_value_parse(p, text(s), &v) == DIVUI_OK);
  assert(divui_element_set_property(n, p, &v) == DIVUI_OK);
  if (v.tag == DIVUI_STYLE_VALUE_COMPOUND)
    assert(divui_style_compound_destroy((DivUiStyleCompound*)v.data.compound) == DIVUI_OK);
}
static DivUiElement* node(DivUiDocument* d, DivUiElement* p) {
  DivUiElement* n = NULL;
  assert(divui_element_create(d, DIVUI_ELEMENT_DIV, &n) == DIVUI_OK);
  assert(divui_element_append_child(p, n) == DIVUI_OK);
  return n;
}
static DivUiTimelineRangeOffsetV1 offset(uint32_t name, double percentage) {
  DivUiTimelineRangeOffsetV1 v;
  memset(&v, 0, sizeof(v));
  v.name = name;
  v.offset.unit = DIVUI_LENGTH_PERCENT;
  v.offset.value = percentage;
  return v;
}
typedef struct SnapshotTail {
  DivUiAnimationAttachmentSnapshotV1 known;
  uint64_t tail;
} SnapshotTail;
typedef struct Callback {
  DivUiElement* port;
  unsigned calls;
} Callback;
static void started(DivUiEvent* event, void* data) {
  Callback* c = (Callback*)data;
  double x, y;
  assert(event->event_type == DIVUI_EVENT_ANIMATION_START);
  assert(divui_element_get_scroll_offset(c->port, &x, &y) == DIVUI_OK);
  ++c->calls;
}
int main(void) {
  DivUiDocumentConfig config = {
      sizeof(config), DIVUI_ABI_VERSION, {320, 240, 320, 240, 1, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* d = NULL;
  DivUiElement* root = NULL;
  assert(divui_document_create(&config, &d) == DIVUI_OK);
  assert(divui_document_root(d, &root) == DIVUI_OK);
  DivUiElement* port = node(d, root);
  style(port, DIVUI_STYLE_PROPERTY_WIDTH, "100px");
  style(port, DIVUI_STYLE_PROPERTY_HEIGHT, "80px");
  style(port, DIVUI_STYLE_PROPERTY_OVERFLOW, "hidden");
  DivUiElement* subject = node(d, port);
  style(subject, DIVUI_STYLE_PROPERTY_WIDTH, "100px");
  style(subject, DIVUI_STYLE_PROPERTY_HEIGHT, "240px");
  style(subject, DIVUI_STYLE_PROPERTY_BACKGROUND_COLOR, "green");
  DivUiElement* target = node(d, root);
  style(target, DIVUI_STYLE_PROPERTY_WIDTH, "20px");
  style(target, DIVUI_STYLE_PROPERTY_HEIGHT, "20px");
  Callback callback = {port, 0};
  DivUiListener* listener = NULL;
  assert(divui_element_add_event_listener(target, DIVUI_EVENT_ANIMATION_START, 0, started,
                                          &callback, &listener) == DIVUI_OK);
  DivUiLinearStopV1 stops[] = {{0, 0}, {29, 0.25}, {29, 0.75}, {100, 1}};
  DivUiEasingCurveV1 curve;
  memset(&curve, 0, sizeof(curve));
  curve.struct_size = sizeof(curve);
  curve.abi_version = DIVUI_ABI_VERSION;
  curve.kind = DIVUI_EASING_CURVE_LINEAR_STOPS_PERCENT;
  curve.stops = stops;
  curve.stop_count = 4;
  curve.stop_stride = sizeof(stops[0]);
  DivUiAnimationOptionsV1 timing;
  memset(&timing, 0, sizeof(timing));
  timing.struct_size = sizeof(timing);
  timing.abi_version = DIVUI_ABI_VERSION;
  timing.duration_ms = 100;
  timing.iterations = 1;
  timing.playback_rate = 1;
  timing.fill = 3;
  timing.easing = &curve;
  DivUiKeyframeV1 frames[2];
  memset(frames, 0, sizeof(frames));
  for (unsigned i = 0; i < 2; ++i) {
    frames[i].struct_size = sizeof(frames[i]);
    frames[i].abi_version = DIVUI_ABI_VERSION;
    frames[i].offset = i;
    frames[i].value.tag = DIVUI_STYLE_VALUE_NUMBER;
    frames[i].value.data.number = (float)i;
  }
  frames[0].easing = &curve;
  DivUiAnimationTimelineSourceV1 source;
  memset(&source, 0, sizeof(source));
  source.struct_size = sizeof(source);
  source.abi_version = DIVUI_ABI_VERSION;
  source.kind = 2;
  source.source = subject;
  source.inset_present = 1;
  source.inset_start.unit = 13;
  source.inset_start.value = 10;
  source.inset_start.calc_offset = 2;
  DivUiAnimationAttachmentRangeV1 range;
  memset(&range, 0, sizeof(range));
  range.struct_size = sizeof(range);
  range.abi_version = DIVUI_ABI_VERSION;
  range.present = 3;
  range.start = offset(2, 0);
  range.end = offset(4, 100);
  uint64_t id = UINT64_MAX;
  assert(divui_element_animate_with_attachment_range_v1(target, DIVUI_STYLE_PROPERTY_OPACITY,
                                                        frames, 2, sizeof(frames[0]), &timing,
                                                        &source, &range, &id) == DIVUI_OK);
  assert(id != UINT64_MAX);
  range.start.offset.value = 999;
  source.inset_start.value = 999;
  stops[1].output = 0;
  frames[1].value.data.number = 0;
  SnapshotTail saved;
  memset(&saved, 0, sizeof(saved));
  saved.known.struct_size = sizeof(saved);
  saved.known.abi_version = DIVUI_ABI_VERSION;
  saved.tail = UINT64_C(0xcafe);
  assert(divui_document_get_animation_attachment_snapshot_v1(d, id, &saved.known) == DIVUI_OK);
  assert(saved.tail == UINT64_C(0xcafe));
  assert(saved.known.present == 3);
  assert(saved.known.range.start.offset.value == 0);
  assert(saved.known.source != NULL);
  assert(divui_element_destroy(saved.known.source) == DIVUI_OK);
  saved.known.source = NULL;
  for (uint32_t name = 0; name < 7; ++name) {
    range.start = offset(name, 0);
    range.end = offset(name, 100);
    assert(divui_document_set_animation_attachment_range_v1(d, id, &range) == DIVUI_OK);
  }
  range.start = offset(0, 0);
  range.end = offset(0, 100);
  assert(divui_document_set_animation_attachment_range_v1(d, id, &range) == DIVUI_OK);
  assert(divui_document_seek_animation_progress_v1(d, id, 29) == DIVUI_OK);
  DivUiAnimationTimelineSnapshotV1 snapshot;
  memset(&snapshot, 0, sizeof(snapshot));
  snapshot.struct_size = sizeof(snapshot);
  snapshot.abi_version = DIVUI_ABI_VERSION;
  assert(divui_document_get_animation_timeline_snapshot_v1(d, id, &snapshot) == DIVUI_OK);
  assert((snapshot.present & 3) == 3);
  assert(snapshot.active == 1);
  assert(snapshot.source != NULL);
  assert(divui_element_destroy(snapshot.source) == DIVUI_OK);
  uint32_t short_header[] = {8, DIVUI_ABI_VERSION};
  id = UINT64_MAX;
  assert(divui_element_animate_with_attachment_range_v1(
             target, DIVUI_STYLE_PROPERTY_OPACITY, frames, 2, sizeof(frames[0]), &timing,
             (const DivUiAnimationTimelineSourceV1*)short_header, NULL,
             &id) == DIVUI_ERROR_INVALID_ARGUMENT);
  assert(id == UINT64_MAX);
  DivUiBuffer* image = NULL;
  assert(divui_document_render_png(d, &image) == DIVUI_OK);
  size_t length = divui_buffer_length(image);
  assert(length > 0);
  assert(divui_listener_destroy(listener) == DIVUI_OK);
  assert(divui_element_destroy(target) == DIVUI_OK);
  assert(divui_element_destroy(subject) == DIVUI_OK);
  assert(divui_element_destroy(port) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(d) == DIVUI_OK);
  assert(saved.known.named[0].end > saved.known.named[0].start);
  assert(divui_buffer_length(image) == length);
  assert(divui_buffer_data(image) != NULL);
  assert(divui_buffer_destroy(image) == DIVUI_OK);
  return 0;
}
