/* Native C11/C++17 consumer of arbitrary owned easing inputs and shared clocks.
 * Run guards in packaged consumers, including optimized builds. */
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include "divui.h"

#ifdef __cplusplus
#define DIVUI_ASSERT_LAYOUT(t, n) static_assert(sizeof(t) == (n), #t " ABI size")
#else
#define DIVUI_ASSERT_LAYOUT(t, n) _Static_assert(sizeof(t) == (n), #t " ABI size")
#endif
DIVUI_ASSERT_LAYOUT(DivUiEasing, 48);
DIVUI_ASSERT_LAYOUT(DivUiAnimationOptions, 104);
DIVUI_ASSERT_LAYOUT(DivUiKeyframe, 80);
DIVUI_ASSERT_LAYOUT(DivUiLinearStopV1, 16);
DIVUI_ASSERT_LAYOUT(DivUiEasingCurveV1, 80);
DIVUI_ASSERT_LAYOUT(DivUiAnimationOptionsV1, 64);
DIVUI_ASSERT_LAYOUT(DivUiKeyframeV1, 40);

typedef struct PointTail {
  DivUiLinearStopV1 point;
  uint64_t tail;
} PointTail;
typedef struct CurveTail {
  DivUiEasingCurveV1 curve;
  uint64_t tail;
} CurveTail;
typedef struct OptionsTail {
  DivUiAnimationOptionsV1 options;
  uint64_t tail;
} OptionsTail;
typedef struct FrameTail {
  DivUiKeyframeV1 frame;
  uint64_t tail;
} FrameTail;

static DivUiUtf8 text(const char* value) {
  DivUiUtf8 result = {(const uint8_t*)value, strlen(value)};
  return result;
}
static void property(DivUiElement* element, DivUiStyleProperty id, const char* literal) {
  DivUiStyleValue value;
  assert(divui_style_value_parse(id, text(literal), &value) == DIVUI_OK);
  assert(divui_element_set_property(element, id, &value) == DIVUI_OK);
  if (value.tag == DIVUI_STYLE_VALUE_COMPOUND)
    assert(divui_style_compound_destroy((DivUiStyleCompound*)value.data.compound) == DIVUI_OK);
}
static DivUiEasingCurveV1 curve(uint32_t kind,
                                const DivUiLinearStopV1* points,
                                size_t count,
                                size_t stride) {
  DivUiEasingCurveV1 value;
  memset(&value, 0, sizeof(value));
  value.struct_size = sizeof(value);
  value.abi_version = DIVUI_ABI_VERSION;
  value.kind = kind;
  value.stops = points;
  value.stop_count = count;
  value.stop_stride = stride;
  return value;
}
static DivUiAnimationOptionsV1 options(const DivUiEasingCurveV1* easing) {
  DivUiAnimationOptionsV1 value;
  memset(&value, 0, sizeof(value));
  value.struct_size = sizeof(value);
  value.abi_version = DIVUI_ABI_VERSION;
  value.duration_ms = 100;
  value.iterations = 1;
  value.playback_rate = 1;
  value.fill = 3;
  value.easing = easing;
  return value;
}
static DivUiKeyframeV1 frame(double offset, float width, const DivUiEasingCurveV1* easing) {
  DivUiKeyframeV1 value;
  memset(&value, 0, sizeof(value));
  value.struct_size = sizeof(value);
  value.abi_version = DIVUI_ABI_VERSION;
  value.offset = offset;
  value.value.tag = DIVUI_STYLE_VALUE_LENGTH;
  value.value.data.length.value = width;
  value.value.data.length.unit = DIVUI_LENGTH_PX;
  value.easing = easing;
  return value;
}
static DivUiElement* node(DivUiDocument* document, DivUiElement* parent) {
  DivUiElement* result = NULL;
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &result) == DIVUI_OK);
  assert(divui_element_append_child(parent, result) == DIVUI_OK);
  return result;
}
static float width(DivUiElement* element) {
  DivUiRect bounds;
  assert(divui_element_get_bounds(element, &bounds) == DIVUI_OK);
  return bounds.width;
}
static void scroll_offset(DivUiElement* element, double x, double y) {
  double actual_x = -1, actual_y = -1;
  assert(divui_element_get_scroll_offset(element, &actual_x, &actual_y) == DIVUI_OK);
  assert(actual_x == x && actual_y == y);
}
typedef struct Callback {
  DivUiDocument* document;
  unsigned starts;
} Callback;
static void started(DivUiEvent* event, void* data) {
  Callback* callback = (Callback*)data;
  uint8_t active = 0;
  assert(event->event_type == DIVUI_EVENT_ANIMATION_START);
  assert(divui_document_is_animating(callback->document, &active) == DIVUI_OK);
  assert(active == 1);
  ++callback->starts;
}

int main(void) {
  DivUiDocumentConfig config = {
      sizeof(config), DIVUI_ABI_VERSION, {320, 240, 320, 240, 1, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  DivUiElement* root = NULL;
  assert(divui_document_create(&config, &document) == DIVUI_OK);
  assert(divui_document_root(document, &root) == DIVUI_OK);
  DivUiElement* animated = node(document, root);
  DivUiElement* linear_segment = node(document, root);
  property(animated, DIVUI_STYLE_PROPERTY_HEIGHT, "20px");
  property(linear_segment, DIVUI_STYLE_PROPERTY_HEIGHT, "20px");
  Callback callback = {document, 0};
  DivUiListener* listener = NULL;
  assert(divui_element_add_event_listener(animated, DIVUI_EVENT_ANIMATION_START, 0, started,
                                          &callback, &listener) == DIVUI_OK);

  PointTail effect_points[] = {{{0, 0}, 11}, {{0.5, 0.25}, 12}, {{1, 1}, 13}};
  DivUiLinearStopV1 segment_points[] = {{0, 0}, {0.25, 0.5}, {1, 1}};
  CurveTail effect;
  effect.curve =
      curve(DIVUI_EASING_CURVE_LINEAR_STOPS, &effect_points[0].point, 3, sizeof(PointTail));
  effect.curve.struct_size = sizeof(effect);
  effect.tail = 21;
  DivUiEasingCurveV1 segment =
      curve(DIVUI_EASING_CURVE_LINEAR_STOPS, segment_points, 3, sizeof(segment_points[0]));
  OptionsTail timing;
  timing.options = options(&effect.curve);
  timing.options.struct_size = sizeof(timing);
  timing.tail = 22;
  FrameTail frames[] = {{frame(0, 20, &segment), 31}, {frame(1, 100, NULL), 32}};
  frames[0].frame.struct_size = frames[1].frame.struct_size = sizeof(FrameTail);
  uint64_t id = UINT64_MAX;
  assert(divui_element_animate_v1(animated, DIVUI_STYLE_PROPERTY_WIDTH, &frames[0].frame, 2,
                                  sizeof(FrameTail), &timing.options, NULL, &id) == DIVUI_OK);
  assert(id != UINT64_MAX && callback.starts == 1);
  DivUiKeyframeV1 defaults[] = {frame(0, 20, NULL), frame(1, 100, NULL)};
  assert(divui_element_animate_v1(linear_segment, DIVUI_STYLE_PROPERTY_WIDTH, defaults, 2,
                                  sizeof(defaults[0]), &timing.options, NULL, &id) == DIVUI_OK);
  /* Mutating caller inputs cannot change either already-owned native effect. */
  effect_points[1].point.output = 0.9;
  segment_points[1].output = 0.1;
  frames[1].frame.value.data.length.value = 20;
  assert(divui_document_set_animation_time(document, 50) == DIVUI_OK);
  assert(width(animated) == 60); /* effect .5 -> .25; segment .25 -> .5 */
  assert(width(linear_segment) == 40);
  assert(effect_points[1].tail == 12 && effect.tail == 21 && timing.tail == 22);
  uint32_t short_header[] = {8, DIVUI_ABI_VERSION};
  id = UINT64_MAX;
  assert(divui_element_animate_v1(animated, DIVUI_STYLE_PROPERTY_WIDTH, defaults, 2,
                                  sizeof(defaults[0]), (const DivUiAnimationOptionsV1*)short_header,
                                  NULL, &id) == DIVUI_ERROR_INVALID_ARGUMENT);
  assert(id == UINT64_MAX && width(animated) == 60);

  DivUiElement* port = node(document, root);
  property(port, DIVUI_STYLE_PROPERTY_POSITION, "relative");
  property(port, DIVUI_STYLE_PROPERTY_WIDTH, "100px");
  property(port, DIVUI_STYLE_PROPERTY_HEIGHT, "80px");
  property(port, DIVUI_STYLE_PROPERTY_OVERFLOW, "hidden");
  property(port, DIVUI_STYLE_PROPERTY_SCROLLBAR_WIDTH, "none");
  DivUiElement* content = node(document, port);
  property(content, DIVUI_STYLE_PROPERTY_WIDTH, "300px");
  property(content, DIVUI_STYLE_PROPERTY_HEIGHT, "240px");
  DivUiElement* target = node(document, port);
  property(target, DIVUI_STYLE_PROPERTY_POSITION, "absolute");
  property(target, DIVUI_STYLE_PROPERTY_LEFT, "150px");
  property(target, DIVUI_STYLE_PROPERTY_TOP, "120px");
  property(target, DIVUI_STYLE_PROPERTY_WIDTH, "20px");
  property(target, DIVUI_STYLE_PROPERTY_HEIGHT, "20px");
  DivUiLinearStopV1 percent_points[] = {{0, 0}, {29, 0.25}, {29, 0.75}, {100, 1}};
  DivUiEasingCurveV1 percent =
      curve(DIVUI_EASING_CURVE_LINEAR_STOPS_PERCENT, percent_points, 4, sizeof(percent_points[0]));
  assert(divui_element_smooth_scroll_to_v1(port, 100, 80, 100, &percent, &id) == DIVUI_OK);
  percent_points[1].output = 0;
  assert(divui_document_set_animation_time(document, 79) == DIVUI_OK);
  scroll_offset(port, 25, 20); /* 29ms preserves the authored 29% discontinuity */
  assert(divui_document_set_animation_time(document, 150) == DIVUI_OK);
  scroll_offset(port, 100, 80);
  assert(divui_element_scroll_to(port, 0, 0) == DIVUI_OK);
  assert(divui_element_smooth_scroll_into_view_v2(target, DIVUI_SCROLL_START, DIVUI_SCROLL_START,
                                                  DIVUI_SCROLL_CONTAINER_NEAREST, 100,
                                                  NULL) == DIVUI_OK);
  assert(divui_document_set_animation_time(document, 200) == DIVUI_OK);
  scroll_offset(port, 75, 60);
  assert(divui_document_set_animation_time(document, 250) == DIVUI_OK);
  scroll_offset(port, 150, 120);

  /* Authored percent endpoints outside 0..100 are valid native inputs. */
  DivUiLinearStopV1 outer_points[] = {{-100, 0}, {200, 1}};
  DivUiEasingCurveV1 outer =
      curve(DIVUI_EASING_CURVE_LINEAR_STOPS_PERCENT, outer_points, 2, sizeof(outer_points[0]));
  DivUiAnimationOptionsV1 transition = options(&outer);
  transition.duration_ms = 100;
  DivUiStyleValue destination = frame(1, 160, NULL).value;
  assert(divui_element_transition_v1(linear_segment, DIVUI_STYLE_PROPERTY_WIDTH, &destination,
                                     &transition, &id) == DIVUI_OK);
  assert(divui_document_set_animation_time(document, 300) == DIVUI_OK);
  assert(width(linear_segment) == 130);
  percent.stop_count = SIZE_MAX;
  id = UINT64_MAX;
  assert(divui_element_smooth_scroll_to_v1(port, 0, 0, 100, &percent, &id) ==
         DIVUI_ERROR_INVALID_ARGUMENT);
  assert(id == UINT64_MAX);
  scroll_offset(port, 150, 120);

  assert(divui_listener_destroy(listener) == DIVUI_OK);
  assert(divui_element_destroy(target) == DIVUI_OK);
  assert(divui_element_destroy(content) == DIVUI_OK);
  assert(divui_element_destroy(port) == DIVUI_OK);
  assert(divui_element_destroy(linear_segment) == DIVUI_OK);
  assert(divui_element_destroy(animated) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(document) == DIVUI_OK);
  return 0;
}
