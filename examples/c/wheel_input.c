#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <math.h>
#include <stdbool.h>
#include <string.h>

#include "divui.h"

static DivUiUtf8 text(const char* value) {
  DivUiUtf8 result = {(const uint8_t*)value, strlen(value)};
  return result;
}
static void property(DivUiElement* node, DivUiStyleProperty id, const char* literal) {
  DivUiStyleValue value;
  memset(&value, 0, sizeof(value));
  assert(divui_style_value_parse(id, text(literal), &value) == DIVUI_OK);
  assert(divui_element_set_property(node, id, &value) == DIVUI_OK);
  if (value.tag == 6)
    assert(divui_style_compound_destroy((DivUiStyleCompound*)value.data.compound) == DIVUI_OK);
}
static DivUiElement* child(DivUiDocument* document,
                           DivUiElement* parent,
                           const char* width,
                           const char* height) {
  DivUiElement* node = NULL;
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &node) == DIVUI_OK);
  assert(divui_element_append_child(parent, node) == DIVUI_OK);
  property(node, DIVUI_STYLE_PROPERTY_DISPLAY, "block");
  property(node, DIVUI_STYLE_PROPERTY_WIDTH, width);
  property(node, DIVUI_STYLE_PROPERTY_HEIGHT, height);
  return node;
}
typedef struct {
  DivUiElement* source;
  bool cancel;
  unsigned capture_count, bubble_count, scroll_count;
  double before_y;
} Observation;
static void observe(DivUiEvent* event, void* user_data) {
  Observation* data = (Observation*)user_data;
  if (event->event_type == DIVUI_EVENT_SCROLL) {
    ++data->scroll_count;
    return;
  }
  assert(event->event_type == DIVUI_EVENT_WHEEL);
  if (event->phase == 1)
    ++data->capture_count;
  else {
    assert(event->phase == 3);
    ++data->bubble_count;
  }
  double x = -1, y = -1;
  assert(divui_element_get_scroll_offset(data->source, &x, &y) == DIVUI_OK);
  data->before_y = y;
  assert(divui_element_set_attribute(data->source, text("data-wheel"), text("observed")) ==
         DIVUI_OK);
  if (data->cancel)
    event->flags |= DIVUI_EVENT_DEFAULT_PREVENTED;
}
int main(void) {
  const double scales[] = {1, 1.25, 1.5, 1.75, 2};
  for (size_t i = 0; i < sizeof(scales) / sizeof(scales[0]); ++i) {
    DivUiDocumentConfig config;
    memset(&config, 0, sizeof(config));
    config.struct_size = sizeof(config);
    config.abi_version = DIVUI_ABI_VERSION;
    config.viewport.logical_width = 320;
    config.viewport.logical_height = 240;
    config.viewport.physical_width = (uint32_t)(320 * scales[i]);
    config.viewport.physical_height = (uint32_t)(240 * scales[i]);
    config.viewport.device_scale_factor = scales[i];
    config.viewport.authority = 1;
    DivUiDocument* document = NULL;
    DivUiElement* root = NULL;
    assert(divui_document_create(&config, &document) == DIVUI_OK);
    assert(divui_document_root(document, &root) == DIVUI_OK);
    DivUiElement* source = child(document, root, "100px", "80px");
    property(source, DIVUI_STYLE_PROPERTY_OVERFLOW, "auto");
    property(source, DIVUI_STYLE_PROPERTY_SCROLLBAR_WIDTH, "none");
    DivUiElement* content = child(document, source, "100px", "240px");
    property(content, DIVUI_STYLE_PROPERTY_BACKGROUND_COLOR, "green");
    DivUiElement* focus = child(document, root, "20px", "20px");
    property(focus, DIVUI_STYLE_PROPERTY_POSITION, "absolute");
    property(focus, DIVUI_STYLE_PROPERTY_LEFT, "200px");
    property(focus, DIVUI_STYLE_PROPERTY_TOP, "0px");
    assert(divui_element_set_attribute(focus, text("tabindex"), text("0")) == DIVUI_OK);
    assert(divui_element_focus(focus) == DIVUI_OK);
    assert(divui_document_update(document) == DIVUI_OK);
    Observation data = {source, true, 0, 0, 0, -1};
    DivUiListener *capture = NULL, *bubble = NULL, *scroll = NULL;
    assert(divui_element_add_event_listener(root, DIVUI_EVENT_WHEEL, 1, observe, &data, &capture) ==
           DIVUI_OK);
    assert(divui_element_add_event_listener(root, DIVUI_EVENT_WHEEL, 0, observe, &data, &bubble) ==
           DIVUI_OK);
    assert(divui_element_add_event_listener(source, DIVUI_EVENT_SCROLL, 0, observe, &data,
                                            &scroll) == DIVUI_OK);
    DivUiRect bounds;
    memset(&bounds, 0, sizeof(bounds));
    assert(divui_element_get_bounds(source, &bounds) == DIVUI_OK);
    DivUiEvent wheel;
    memset(&wheel, 0, sizeof(wheel));
    wheel.struct_size = sizeof(wheel);
    wheel.abi_version = DIVUI_ABI_VERSION;
    wheel.event_type = DIVUI_EVENT_WHEEL;
    wheel.x = bounds.x + 2;
    wheel.y = bounds.y + 2;
    wheel.delta_y = 20;
    wheel.timestamp_ns = 123;
    assert(divui_document_dispatch_wheel_input_v1(document, &wheel) == DIVUI_OK);
    double x = -1, y = -1;
    assert(divui_element_get_scroll_offset(source, &x, &y) == DIVUI_OK);
    assert(x == 0 && y == 0 && data.capture_count == 1 && data.bubble_count == 1);
    assert(wheel.flags == 0 && wheel.timestamp_ns == 123);
    data.cancel = false;
    assert(divui_document_dispatch_wheel_input_v1(document, &wheel) == DIVUI_OK);
    assert(divui_element_get_scroll_offset(source, &x, &y) == DIVUI_OK);
    assert(y == 20 && data.before_y == 0);
    uint32_t pending = UINT32_MAX;
    assert(divui_document_dispatch_pending_events_v1(document, &pending) == DIVUI_OK);
    assert(data.scroll_count == 1);
    DivUiElement* focused = NULL;
    assert(divui_document_focused_element_v1(document, &focused) == DIVUI_OK);
    uint32_t same = 0;
    assert(divui_element_is_same_node_v1(focused, focus, &same) == DIVUI_OK && same == 1);
    assert(divui_element_destroy(focused) == DIVUI_OK);
    DivUiEvent raw = wheel;
    assert(divui_document_dispatch_event(document, content, &raw) == DIVUI_OK);
    assert(divui_element_get_scroll_offset(source, &x, &y) == DIVUI_OK && y == 20);
    wheel.delta_y = NAN;
    assert(divui_document_dispatch_wheel_input_v1(document, &wheel) ==
           DIVUI_ERROR_INVALID_ARGUMENT);
    assert(divui_element_get_scroll_offset(source, &x, &y) == DIVUI_OK && y == 20);
    assert(divui_listener_destroy(capture) == DIVUI_OK);
    assert(divui_listener_destroy(bubble) == DIVUI_OK);
    assert(divui_listener_destroy(scroll) == DIVUI_OK);
    assert(divui_element_destroy(focus) == DIVUI_OK);
    assert(divui_element_destroy(content) == DIVUI_OK);
    assert(divui_element_destroy(source) == DIVUI_OK);
    assert(divui_element_destroy(root) == DIVUI_OK);
    assert(divui_document_destroy(document) == DIVUI_OK);
  }
  return 0;
}
