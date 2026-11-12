/* Native owned radius-axis inputs. Uses the same Engine declarations/effects. */
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <string.h>

#include "divui.h"

static DivUiUtf8 utf8(const char* text) {
  DivUiUtf8 value = {(const uint8_t*)text, strlen(text)};
  return value;
}
static void property(DivUiElement* node, DivUiStyleProperty id, const char* text) {
  DivUiStyleValue value;
  assert(divui_style_value_parse(id, utf8(text), &value) == DIVUI_OK);
  assert(divui_element_set_property(node, id, &value) == DIVUI_OK);
  if (value.tag == DIVUI_STYLE_VALUE_COMPOUND)
    assert(divui_style_compound_destroy((DivUiStyleCompound*)value.data.compound) == DIVUI_OK);
}
static DivUiRadiusAxisLengthV1 length(float value, float offset, uint32_t unit) {
  DivUiRadiusAxisLengthV1 result = {sizeof(result), DIVUI_ABI_VERSION, value, offset, unit, 0};
  return result;
}
static DivUiStyleValue carrier(DivUiStyleProperty axis, DivUiRadiusAxisLengthV1 input) {
  DivUiStyleValue value;
  assert(divui_style_value_radius_axis_length_v1(axis, &input, &value) == DIVUI_OK);
  assert(value.tag == DIVUI_STYLE_VALUE_COMPOUND && value.reserved == 0);
  return value;
}
static void set(DivUiElement* node, DivUiStyleProperty axis, DivUiRadiusAxisLengthV1 input) {
  DivUiStyleValue value = carrier(axis, input);
  assert(divui_element_set_property(node, axis, &value) == DIVUI_OK);
  assert(divui_style_compound_destroy((DivUiStyleCompound*)value.data.compound) == DIVUI_OK);
}
static DivUiRadiusAxisLengthV1 get(DivUiElement* node, DivUiStyleProperty axis) {
  DivUiRadiusAxisLengthV1 value = length(0, 0, 0);
  assert(divui_element_radius_axis_length_v1(node, axis, &value) == DIVUI_OK);
  return value;
}
static void equals(DivUiElement* node,
                   DivUiStyleProperty axis,
                   float value,
                   float offset,
                   uint32_t unit) {
  DivUiRadiusAxisLengthV1 actual = get(node, axis);
  assert(actual.value == value && actual.calc_offset == offset && actual.unit == unit);
}
static void same_pixels(const DivUiBitmap* a, const DivUiBitmap* b) {
  assert(a->width == b->width && a->height == b->height);
  for (uint32_t y = 0; y < a->height; ++y)
    assert(memcmp(divui_buffer_data(a->pixels) + y * a->stride,
                  divui_buffer_data(b->pixels) + y * b->stride, a->width * 4) == 0);
}
static void native_axes(double scale) {
  assert(sizeof(DivUiRadiusAxisLengthV1) == 24);
  DivUiDocumentConfig config = {sizeof(config),
                                DIVUI_ABI_VERSION,
                                {160, 140, (uint32_t)(160 * scale), (uint32_t)(140 * scale), scale,
                                 DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* doc = NULL;
  DivUiElement *root = NULL, *node = NULL, *child = NULL;
  assert(divui_document_create(&config, &doc) == DIVUI_OK);
  assert(divui_document_root(doc, &root) == DIVUI_OK);
  assert(divui_element_create(doc, DIVUI_ELEMENT_DIV, &node) == DIVUI_OK);
  assert(divui_element_create(doc, DIVUI_ELEMENT_DIV, &child) == DIVUI_OK);
  assert(divui_element_append_child(root, node) == DIVUI_OK);
  assert(divui_element_append_child(node, child) == DIVUI_OK);
  property(root, DIVUI_STYLE_PROPERTY_BACKGROUND_COLOR, "white");
  property(node, DIVUI_STYLE_PROPERTY_DISPLAY, "block");
  property(node, DIVUI_STYLE_PROPERTY_WIDTH, "100px");
  property(node, DIVUI_STYLE_PROPERTY_HEIGHT, "80px");
  property(node, DIVUI_STYLE_PROPERTY_OVERFLOW, "hidden");
  property(child, DIVUI_STYLE_PROPERTY_WIDTH, "100px");
  property(child, DIVUI_STYLE_PROPERTY_HEIGHT, "80px");
  property(child, DIVUI_STYLE_PROPERTY_BACKGROUND_COLOR, "green");
  const DivUiStyleProperty axes[] = {DIVUI_STYLE_PROPERTY_BORDER_TOP_LEFT_RADIUS_X,
                                     DIVUI_STYLE_PROPERTY_BORDER_TOP_LEFT_RADIUS_Y,
                                     DIVUI_STYLE_PROPERTY_BORDER_TOP_RIGHT_RADIUS_X,
                                     DIVUI_STYLE_PROPERTY_BORDER_TOP_RIGHT_RADIUS_Y,
                                     DIVUI_STYLE_PROPERTY_BORDER_BOTTOM_RIGHT_RADIUS_X,
                                     DIVUI_STYLE_PROPERTY_BORDER_BOTTOM_RIGHT_RADIUS_Y,
                                     DIVUI_STYLE_PROPERTY_BORDER_BOTTOM_LEFT_RADIUS_X,
                                     DIVUI_STYLE_PROPERTY_BORDER_BOTTOM_LEFT_RADIUS_Y};
  for (size_t i = 0; i < 8; ++i) {
    assert((int)axes[i] == 251 + (int)i);
    set(node, axes[i], length((float)i + 1, 0, DIVUI_LENGTH_PX));
    equals(node, axes[i], (float)i + 1, 0, DIVUI_LENGTH_PX);
  }
  set(node, axes[0], length(25, 5, DIVUI_RADIUS_AXIS_LENGTH_CALCULATED));
  set(node, axes[1], length(25, 0, DIVUI_LENGTH_PERCENT));
  set(node, axes[3], length(1, 0, DIVUI_LENGTH_EM));
  equals(node, axes[0], 25, 5, DIVUI_RADIUS_AXIS_LENGTH_CALCULATED);
  equals(node, axes[3], 16, 0, DIVUI_LENGTH_PX);
  DivUiBitmap source = {sizeof(source), DIVUI_ABI_VERSION, 0, 0, 0, NULL};
  assert(divui_document_render_rgba(doc, &source) == DIVUI_OK);
  set(node, axes[0], length(30, 0, DIVUI_LENGTH_PX));
  set(node, axes[1], length(20, 0, DIVUI_LENGTH_PX));
  set(node, axes[3], length(16, 0, DIVUI_LENGTH_PX));
  DivUiBitmap fixed = {sizeof(fixed), DIVUI_ABI_VERSION, 0, 0, 0, NULL};
  assert(divui_document_render_rgba(doc, &fixed) == DIVUI_OK);
  same_pixels(&source, &fixed);
  set(node, axes[0], length(25, 0, DIVUI_LENGTH_PERCENT));
  DivUiKeyframeV1 frames[2];
  memset(frames, 0, sizeof(frames));
  for (size_t i = 0; i < 2; ++i) {
    frames[i].struct_size = sizeof(frames[i]);
    frames[i].abi_version = DIVUI_ABI_VERSION;
    frames[i].offset = (double)i;
    frames[i].value = carrier(axes[0], length(5, 0, DIVUI_LENGTH_PX));
  }
  DivUiAnimationOptionsV1 options;
  memset(&options, 0, sizeof(options));
  options.struct_size = sizeof(options);
  options.abi_version = DIVUI_ABI_VERSION;
  options.duration_ms = 100;
  options.iterations = 1;
  options.playback_rate = 1;
  options.fill = 3;
  options.composite = 1;
  DivUiAnimationTimeline timeline;
  memset(&timeline, 0, sizeof(timeline));
  timeline.struct_size = sizeof(timeline);
  timeline.abi_version = DIVUI_ABI_VERSION;
  uint64_t animation = 0;
  assert(divui_element_animate_v1(node, axes[0], frames, 2, sizeof(frames[0]), &options, &timeline,
                                  &animation) == DIVUI_OK);
  for (size_t i = 0; i < 2; ++i)
    assert(divui_style_compound_destroy((DivUiStyleCompound*)frames[i].value.data.compound) ==
           DIVUI_OK);
  equals(node, axes[0], 25, 5, DIVUI_RADIUS_AXIS_LENGTH_CALCULATED);
  set(node, axes[0], length(40, 0, DIVUI_LENGTH_PERCENT));
  equals(node, axes[0], 40, 5, DIVUI_RADIUS_AXIS_LENGTH_CALCULATED);
  assert(divui_document_animation_cancel(doc, animation) == DIVUI_OK);
  equals(node, axes[0], 40, 0, DIVUI_LENGTH_PERCENT);
  uint8_t removed = 255;
  assert(divui_element_remove_property(node, axes[0], &removed) == DIVUI_OK && removed == 1);
  equals(node, axes[0], 0, 0, DIVUI_LENGTH_PX);
  equals(node, axes[1], 20, 0, DIVUI_LENGTH_PX);
  assert(divui_element_remove_property(node, axes[0], &removed) == DIVUI_OK && removed == 0);
  struct Future {
    DivUiRadiusAxisLengthV1 prefix;
    uint32_t tail[2];
  } future;
  memset(&future, 0, sizeof(future));
  future.prefix = length(0, 0, 0);
  future.prefix.struct_size = sizeof(future);
  future.tail[0] = 123;
  future.tail[1] = 456;
  assert(divui_element_radius_axis_length_v1(node, axes[1], &future.prefix) == DIVUI_OK);
  assert(future.prefix.value == 20 && future.tail[0] == 123 && future.tail[1] == 456);
  DivUiRadiusAxisLengthV1 bad = length(10, 0, DIVUI_LENGTH_AUTO);
  DivUiStyleValue output;
  memset(&output, 0x5a, sizeof(output));
  const DivUiStyleValue before = output;
  assert(divui_style_value_radius_axis_length_v1(axes[0], &bad, &output) ==
         DIVUI_ERROR_INVALID_ARGUMENT);
  assert(memcmp(&output, &before, sizeof(output)) == 0);
  assert(divui_element_destroy(child) == DIVUI_OK);
  assert(divui_element_destroy(node) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(doc) == DIVUI_OK);
  assert(divui_buffer_data(source.pixels) != NULL);
  assert(divui_buffer_data(fixed.pixels) != NULL);
  same_pixels(&source, &fixed);
  assert(divui_buffer_destroy(source.pixels) == DIVUI_OK);
  assert(divui_buffer_destroy(fixed.pixels) == DIVUI_OK);
}
int main(void) {
  const double scales[] = {1, 1.25, 1.5, 1.75, 2};
  for (size_t i = 0; i < 5; ++i)
    native_axes(scales[i]);
  return 0;
}
