/* Keep consumer checks active in optimized SDK builds. */
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "divui.h"

static DivUiUtf8 utf8(const char* text) {
  DivUiUtf8 value = {(const uint8_t*)text, strlen(text)};
  return value;
}
static void property(DivUiElement* element, DivUiStyleProperty id, const char* text) {
  DivUiStyleValue value;
  assert(divui_style_value_parse(id, utf8(text), &value) == DIVUI_OK);
  assert(divui_element_set_property(element, id, &value) == DIVUI_OK);
  if (value.tag == DIVUI_STYLE_VALUE_COMPOUND)
    assert(divui_style_compound_destroy((DivUiStyleCompound*)value.data.compound) == DIVUI_OK);
}
static void offset(DivUiElement* element, double expected_x, double expected_y) {
  double x = NAN, y = NAN;
  assert(divui_element_get_scroll_offset(element, &x, &y) == DIVUI_OK);
  assert(x == expected_x && y == expected_y);
}
static void scenario(double scale) {
  DivUiDocumentConfig config = {sizeof(config),
                                DIVUI_ABI_VERSION,
                                {320.0, 240.0, (uint32_t)(320.0 * scale), (uint32_t)(240.0 * scale),
                                 scale, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  DivUiElement* root = NULL;
  DivUiElement* source = NULL;
  DivUiElement* content = NULL;
  assert(divui_document_create(&config, &document) == DIVUI_OK);
  assert(divui_document_root(document, &root) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &source) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &content) == DIVUI_OK);
  assert(divui_element_append_child(root, source) == DIVUI_OK);
  assert(divui_element_append_child(source, content) == DIVUI_OK);
  property(source, DIVUI_STYLE_PROPERTY_DISPLAY, "block");
  property(source, DIVUI_STYLE_PROPERTY_WIDTH, "100px");
  property(source, DIVUI_STYLE_PROPERTY_HEIGHT, "80px");
  property(source, DIVUI_STYLE_PROPERTY_OVERFLOW, "hidden");
  property(source, DIVUI_STYLE_PROPERTY_SCROLLBAR_WIDTH, "none");
  property(content, DIVUI_STYLE_PROPERTY_DISPLAY, "block");
  property(content, DIVUI_STYLE_PROPERTY_WIDTH, "300px");
  property(content, DIVUI_STYLE_PROPERTY_HEIGHT, "280px");
  assert(divui_element_scroll_to(source, 40.0, 40.0) == DIVUI_OK);
  const double points[] = {0.0, 100.0};
  DivUiEasingCurveV1 curve;
  memset(&curve, 0, sizeof(curve));
  curve.struct_size = sizeof(curve);
  curve.abi_version = DIVUI_ABI_VERSION;
  curve.kind = 2;          /* steps */
  curve.step_position = 1; /* jump-end */
  curve.step_count = 2;
  uint64_t id = UINT64_MAX;
  uint8_t found = 255;
  assert(divui_element_settle_scroll_snap_with_easing_v1(source, points, 2, NULL, 0, 100.0, &curve,
                                                         &id, &found) == DIVUI_OK);
  assert(found == 1 && id != UINT64_MAX && id != 0);
  assert(divui_document_set_animation_time(document, 25.0) == DIVUI_OK);
  offset(source, 40.0, 40.0);
  assert(divui_document_set_animation_time(document, 50.0) == DIVUI_OK);
  offset(source, 20.0, 40.0);
  assert(divui_document_cancel_smooth_scroll_v1(document, id) == DIVUI_OK);
  offset(source, 20.0, 40.0);
  assert(divui_document_cancel_smooth_scroll_v1(document, id) == DIVUI_ERROR_INVALID_STATE);
  assert(divui_document_set_animation_time(document, 100.0) == DIVUI_OK);
  offset(source, 20.0, 40.0);
  id = UINT64_MAX;
  found = 255;
  assert(divui_element_settle_scroll_snap_with_easing_v1(source, NULL, 0, NULL, 0, 100.0, NULL, &id,
                                                         &found) == DIVUI_OK);
  assert(id == 0 && found == 0);
  id = UINT64_MAX;
  found = 255;
  assert(divui_element_settle_scroll_snap_with_easing_v1(source, NULL, 0, NULL, 0, -1.0, NULL, &id,
                                                         &found) == DIVUI_ERROR_INVALID_ARGUMENT);
  assert(id == UINT64_MAX && found == 255);
  offset(source, 20.0, 40.0);
  assert(divui_element_destroy(content) == DIVUI_OK);
  assert(divui_element_destroy(source) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(document) == DIVUI_OK);
}
int main(void) {
  const double scales[] = {1.0, 1.25, 1.5, 1.75, 2.0};
  for (size_t index = 0; index < sizeof(scales) / sizeof(scales[0]); ++index)
    scenario(scales[index]);
  puts("native explicit snap/cancel C ABI: passed");
  return 0;
}
