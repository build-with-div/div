/* C11/C++17 native app: source reassignment uses the shared Engine controls. */
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stdint.h>
#include <string.h>

#include "divui.h"
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
static DivUiElement* port(DivUiDocument* d, DivUiElement* root, DivUiElement** content) {
  DivUiElement* p = node(d, root);
  style(p, DIVUI_STYLE_PROPERTY_WIDTH, "100px");
  style(p, DIVUI_STYLE_PROPERTY_HEIGHT, "80px");
  style(p, DIVUI_STYLE_PROPERTY_OVERFLOW, "hidden");
  *content = node(d, p);
  style(*content, DIVUI_STYLE_PROPERTY_WIDTH, "100px");
  style(*content, DIVUI_STYLE_PROPERTY_HEIGHT, "240px");
  return p;
}
static DivUiAnimationTimelineSourceV1 source(DivUiElement* n) {
  DivUiAnimationTimelineSourceV1 v;
  memset(&v, 0, sizeof(v));
  v.struct_size = sizeof(v);
  v.abi_version = DIVUI_ABI_VERSION;
  v.kind = 1;
  v.source = n;
  return v;
}
static DivUiAnimationState state(DivUiDocument* d, uint64_t id) {
  DivUiAnimationState v;
  memset(&v, 0, sizeof(v));
  v.struct_size = sizeof(v);
  v.abi_version = DIVUI_ABI_VERSION;
  assert(divui_document_animation_get_state(d, id, &v) == DIVUI_OK);
  return v;
}
static double percentage(DivUiDocument* d, uint64_t id) {
  DivUiAnimationTimelineSnapshotV1 v;
  memset(&v, 0, sizeof(v));
  v.struct_size = sizeof(v);
  v.abi_version = DIVUI_ABI_VERSION;
  assert(divui_document_get_animation_timeline_snapshot_v1(d, id, &v) == DIVUI_OK);
  assert(v.active == 1);
  assert(v.source != NULL);
  assert(divui_element_destroy(v.source) == DIVUI_OK);
  return v.current_time_percentage;
}
static uint64_t animate(DivUiElement* target,
                        DivUiElement* p,
                        const DivUiAnimationAttachmentRangeV1* range) {
  DivUiKeyframeV1 frames[2];
  memset(frames, 0, sizeof(frames));
  for (unsigned i = 0; i < 2; ++i) {
    frames[i].struct_size = sizeof(frames[i]);
    frames[i].abi_version = DIVUI_ABI_VERSION;
    frames[i].offset = i;
    frames[i].value.tag = DIVUI_STYLE_VALUE_NUMBER;
    frames[i].value.data.number = (float)i;
  }
  DivUiAnimationOptionsV1 options;
  memset(&options, 0, sizeof(options));
  options.struct_size = sizeof(options);
  options.abi_version = DIVUI_ABI_VERSION;
  options.duration_ms = 1000;
  options.iterations = 1;
  options.playback_rate = 1;
  options.fill = 3;
  DivUiAnimationTimelineSourceV1 s = source(p);
  uint64_t id = UINT64_MAX;
  assert(divui_element_animate_with_attachment_range_v1(target, DIVUI_STYLE_PROPERTY_OPACITY,
                                                        frames, 2, sizeof(frames[0]), &options, &s,
                                                        range, &id) == DIVUI_OK);
  return id;
}
typedef struct Reentry {
  DivUiDocument* document;
  uint64_t id;
  unsigned calls;
  DivUiStatus set_status;
  DivUiStatus pause_status;
} Reentry;
static void started(DivUiEvent* event, void* data) {
  Reentry* c = (Reentry*)data;
  assert(event->event_type == DIVUI_EVENT_ANIMATION_START);
  ++c->calls;
  c->set_status = divui_document_set_animation_timeline_source_v1(c->document, c->id, NULL);
  c->pause_status = divui_document_animation_pause(c->document, c->id);
}
int main(void) {
  DivUiDocumentConfig config = {
      sizeof(config), DIVUI_ABI_VERSION, {400, 400, 400, 400, 1, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* d = NULL;
  DivUiElement* root = NULL;
  assert(divui_document_create(&config, &d) == DIVUI_OK);
  assert(divui_document_root(d, &root) == DIVUI_OK);
  DivUiElement *ac = NULL, *bc = NULL;
  DivUiElement *a = port(d, root, &ac), *b = port(d, root, &bc), *target = node(d, root);
  style(target, DIVUI_STYLE_PROPERTY_WIDTH, "20px");
  style(target, DIVUI_STYLE_PROPERTY_HEIGHT, "20px");
  style(target, DIVUI_STYLE_PROPERTY_BACKGROUND_COLOR, "blue");
  assert(divui_element_scroll_to(a, 0, 40) == DIVUI_OK);
  assert(divui_element_scroll_to(b, 0, 120) == DIVUI_OK);
  uint64_t id = animate(target, a, NULL);
  assert(divui_document_seek_animation_progress_v1(d, id, 50) == DIVUI_OK);
  DivUiAnimationTimelineSourceV1 same = source(a);
  assert(divui_document_set_animation_timeline_source_v1(d, id, &same) == DIVUI_OK);
  assert(percentage(d, id) == 50);
  typedef struct Future {
    DivUiAnimationTimelineSourceV1 known;
    uint64_t tail;
  } Future;
  Future next;
  memset(&next, 0, sizeof(next));
  next.known = source(b);
  next.known.struct_size = sizeof(next);
  next.tail = UINT64_C(0xcafe);
  Future before = next;
  assert(divui_document_set_animation_timeline_source_v1(d, id, &next.known) == DIVUI_OK);
  assert(memcmp(&next, &before, sizeof(next)) == 0);
  assert(percentage(d, id) == 75);
  next.known.kind = 99;
  assert(divui_document_set_animation_timeline_source_v1(d, id, &next.known) ==
         DIVUI_ERROR_INVALID_ARGUMENT);
  assert(percentage(d, id) == 75);
  assert(divui_document_animation_pause(d, id) == DIVUI_OK);
  assert(divui_document_set_animation_timeline_source_v1(d, id, NULL) == DIVUI_OK);
  DivUiAnimationState held = state(d, id);
  assert(held.current_time_ms == 750);
  assert(held.play_state == 1);
  DivUiAnimationTimelineSnapshotV1 timeline;
  memset(&timeline, 0, sizeof(timeline));
  timeline.struct_size = sizeof(timeline);
  timeline.abi_version = DIVUI_ABI_VERSION;
  assert(divui_document_get_animation_timeline_snapshot_v1(d, id, &timeline) == DIVUI_OK);
  assert(timeline.present == 0 && timeline.source == NULL);
  DivUiAnimationAttachmentSnapshotV1 attachment;
  memset(&attachment, 0, sizeof(attachment));
  attachment.struct_size = sizeof(attachment);
  attachment.abi_version = DIVUI_ABI_VERSION;
  assert(divui_document_get_animation_attachment_snapshot_v1(d, id, &attachment) == DIVUI_OK);
  assert(attachment.present == 1 && attachment.source == NULL);
  assert(divui_document_animation_cancel(d, id) == DIVUI_OK);
  assert(divui_element_scroll_to(a, 0, 0) == DIVUI_OK);
  DivUiAnimationAttachmentRangeV1 range;
  memset(&range, 0, sizeof(range));
  range.struct_size = sizeof(range);
  range.abi_version = DIVUI_ABI_VERSION;
  range.present = 1;
  range.start.name = 6;
  range.start.offset.unit = DIVUI_LENGTH_PERCENT;
  range.start.offset.value = 50;
  id = animate(target, a, &range);
  Reentry callback = {d, id, 0, DIVUI_ERROR_INTERNAL, DIVUI_ERROR_INTERNAL};
  DivUiListener* listener = NULL;
  assert(divui_element_add_event_listener(target, DIVUI_EVENT_ANIMATION_START, 0, started,
                                          &callback, &listener) == DIVUI_OK);
  next.known = source(b);
  assert(divui_document_set_animation_timeline_source_v1(d, id, &next.known) == DIVUI_OK);
  assert(callback.calls == 1 && callback.set_status == DIVUI_OK &&
         callback.pause_status == DIVUI_OK);
  held = state(d, id);
  /* Preserve source-clock subtraction before EffectEnd division and ms reflection. */
  const double duration = (160.0 * 16.0 / 1000.0) / 1000.0;
  const double current = (1920.0 / 1000000.0) - 0.5 * duration;
  const double expected_hold_ms = (current / (duration * 0.5)) * 1000.0;
  assert(held.current_time_ms == expected_hold_ms && held.play_state == 1);
  assert(divui_listener_destroy(listener) == DIVUI_OK);
  DivUiBuffer* image = NULL;
  assert(divui_document_render_png(d, &image) == DIVUI_OK);
  size_t length = divui_buffer_length(image);
  assert(length > 0);
  assert(divui_element_destroy(target) == DIVUI_OK);
  assert(divui_element_destroy(ac) == DIVUI_OK);
  assert(divui_element_destroy(bc) == DIVUI_OK);
  assert(divui_element_destroy(a) == DIVUI_OK);
  assert(divui_element_destroy(b) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(d) == DIVUI_OK);
  assert(divui_buffer_length(image) == length);
  assert(divui_buffer_data(image) != NULL);
  assert(divui_buffer_destroy(image) == DIVUI_OK);
  return 0;
}
