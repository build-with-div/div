/* Native SVG viewport mutation through the public C API and event pipeline. */
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "divui.h"

static DivUiUtf8 svg_utf8(const char* value) {
  DivUiUtf8 result = {(const uint8_t*)value, strlen(value)};
  return result;
}

static void svg_property(DivUiElement* element, DivUiStyleProperty property, const char* literal) {
  DivUiStyleValue value;
  DivUiStatus status = divui_style_value_parse(property, svg_utf8(literal), &value);
  if (status != DIVUI_OK)
    fprintf(stderr, "native SVG property %d literal '%s': %d\n", property, literal, status);
  assert(status == DIVUI_OK);
  assert(divui_element_set_property(element, property, &value) == DIVUI_OK);
  if (value.tag == DIVUI_STYLE_VALUE_COMPOUND)
    assert(divui_style_compound_destroy((DivUiStyleCompound*)value.data.compound) == DIVUI_OK);
}

typedef struct SvgCallbackState {
  DivUiElement* viewport;
  DivUiElement* ordinary;
  unsigned calls;
} SvgCallbackState;

static void svg_clicked(DivUiEvent* event, void* data) {
  SvgCallbackState* state = (SvgCallbackState*)data;
  assert(event->event_type == DIVUI_EVENT_CLICK);
  svg_property(state->viewport, DIVUI_STYLE_PROPERTY_BORDER_LEFT, "3px double black");
  svg_property(state->viewport, DIVUI_STYLE_PROPERTY_BORDER_RADIUS, "7px");
  svg_property(state->viewport, DIVUI_STYLE_PROPERTY_PADDING, "2px");
  svg_property(state->viewport, DIVUI_STYLE_PROPERTY_BOX_SIZING, "border-box");
  svg_property(state->ordinary, DIVUI_STYLE_PROPERTY_BOX_SIZING, "border-box");
  ++state->calls;
}

static void svg_viewport(double scale,
                         const char* width,
                         const char* height,
                         float expected_width,
                         float expected_height) {
  DivUiDocumentConfig config = {sizeof(config),
                                DIVUI_ABI_VERSION,
                                {320.0, 240.0, (uint32_t)(320.0 * scale), (uint32_t)(240.0 * scale),
                                 scale, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  DivUiElement* root = NULL;
  DivUiElement* svg = NULL;
  DivUiElement* viewport = NULL;
  DivUiElement* ordinary = NULL;
  assert(divui_document_create(&config, &document) == DIVUI_OK);
  assert(divui_document_root(document, &root) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_SVG, &svg) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_SVG_FOREIGN_OBJECT, &viewport) == DIVUI_OK);
  assert(divui_element_append_child(root, svg) == DIVUI_OK);
  assert(divui_element_append_child(svg, viewport) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &ordinary) == DIVUI_OK);
  assert(divui_element_append_child(root, ordinary) == DIVUI_OK);
  svg_property(root, DIVUI_STYLE_PROPERTY_BACKGROUND_COLOR, "white");
  svg_property(svg, DIVUI_STYLE_PROPERTY_POSITION, "absolute");
  svg_property(svg, DIVUI_STYLE_PROPERTY_LEFT, "20px");
  svg_property(svg, DIVUI_STYLE_PROPERTY_TOP, "20px");
  svg_property(svg, DIVUI_STYLE_PROPERTY_WIDTH, "240px");
  svg_property(svg, DIVUI_STYLE_PROPERTY_HEIGHT, "180px");
  svg_property(svg, DIVUI_STYLE_PROPERTY_OVERFLOW, "hidden");
  svg_property(viewport, DIVUI_STYLE_PROPERTY_WIDTH, width);
  svg_property(viewport, DIVUI_STYLE_PROPERTY_HEIGHT, height);
  svg_property(ordinary, DIVUI_STYLE_PROPERTY_POSITION, "absolute");
  svg_property(ordinary, DIVUI_STYLE_PROPERTY_LEFT, "120px");
  svg_property(ordinary, DIVUI_STYLE_PROPERTY_TOP, "20px");
  svg_property(ordinary, DIVUI_STYLE_PROPERTY_WIDTH, "24px");
  svg_property(ordinary, DIVUI_STYLE_PROPERTY_HEIGHT, "18px");
  svg_property(ordinary, DIVUI_STYLE_PROPERTY_BORDER_LEFT, "3px solid black");
  svg_property(ordinary, DIVUI_STYLE_PROPERTY_PADDING, "2px");
  svg_property(ordinary, DIVUI_STYLE_PROPERTY_BOX_SIZING, "content-box");
  DivUiRect before;
  assert(divui_element_get_bounds(viewport, &before) == DIVUI_OK);
  assert(before.x == 20.0f && before.y == 20.0f);
  assert(before.width == expected_width && before.height == expected_height);
  DivUiRect ordinary_before;
  assert(divui_element_get_bounds(ordinary, &ordinary_before) == DIVUI_OK);
  assert(ordinary_before.width == 31.0f && ordinary_before.height == 22.0f);

  SvgCallbackState state = {viewport, ordinary, 0};
  DivUiListener* listener = NULL;
  assert(divui_element_add_event_listener(viewport, DIVUI_EVENT_CLICK, 0, svg_clicked, &state,
                                          &listener) == DIVUI_OK);
  DivUiEvent event;
  memset(&event, 0, sizeof(event));
  event.struct_size = sizeof(event);
  event.abi_version = DIVUI_ABI_VERSION;
  event.event_type = DIVUI_EVENT_CLICK;
  assert(divui_document_dispatch_event(document, viewport, &event) == DIVUI_OK);
  assert(state.calls == 1);
  DivUiRect after;
  assert(divui_element_get_bounds(viewport, &after) == DIVUI_OK);
  assert(after.x == before.x && after.y == before.y);
  assert(after.width == expected_width && after.height == expected_height);
  assert(before.width == expected_width && before.height == expected_height);
  DivUiRect ordinary_after;
  assert(divui_element_get_bounds(ordinary, &ordinary_after) == DIVUI_OK);
  assert(ordinary_after.x == ordinary_before.x && ordinary_after.y == ordinary_before.y);
  assert(ordinary_after.width == 24.0f && ordinary_after.height == 18.0f);
  assert(ordinary_before.width == 31.0f && ordinary_before.height == 22.0f);
  DivUiBuffer* png = NULL;
  assert(divui_document_render_png(document, &png) == DIVUI_OK);
  assert(divui_buffer_length(png) > 0);
  assert(divui_buffer_destroy(png) == DIVUI_OK);

  assert(divui_element_detach(viewport) == DIVUI_OK);
  size_t count = 1;
  assert(divui_element_get_client_rects_v1(viewport, NULL, 0, &count) == DIVUI_OK && count == 0);
  assert(divui_element_append_child(svg, viewport) == DIVUI_OK);
  assert(divui_element_get_bounds(viewport, &after) == DIVUI_OK);
  assert(after.width == expected_width && after.height == expected_height);
  /* Callback data remains live until the owned listener is destroyed. */
  assert(divui_listener_destroy(listener) == DIVUI_OK);
  assert(divui_element_destroy(viewport) == DIVUI_OK);
  assert(divui_element_destroy(ordinary) == DIVUI_OK);
  assert(divui_element_destroy(svg) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(document) == DIVUI_OK);
}

int main(void) {
  const double scales[] = {1.0, 1.25, 1.5, 2.0, 3.0};
  for (size_t i = 0; i < sizeof(scales) / sizeof(scales[0]); ++i) {
    svg_viewport(scales[i], "1px", "1px", 1.0f, 1.0f);
    svg_viewport(scales[i], "24px", "18px", 24.0f, 18.0f);
  }
  puts("native SVG viewport: bounds, mutation callbacks, detach/reattach and teardown passed");
  return 0;
}
