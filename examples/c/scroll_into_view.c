/* Consuming C app. Checks remain active in release SDK builds. */
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <math.h>
#include <string.h>

#include "divui.h"

static void property(DivUiElement* e, DivUiStyleProperty id, const char* literal) {
  DivUiUtf8 text = {(const uint8_t*)literal, strlen(literal)};
  DivUiStyleValue value;
  assert(divui_style_value_parse(id, text, &value) == DIVUI_OK);
  assert(divui_element_set_property(e, id, &value) == DIVUI_OK);
  if (value.tag == DIVUI_STYLE_VALUE_COMPOUND)
    assert(divui_style_compound_destroy((DivUiStyleCompound*)value.data.compound) == DIVUI_OK);
}

static DivUiElement* block(DivUiDocument* d, DivUiElement* parent, const char* w, const char* h) {
  DivUiElement* e = NULL;
  assert(divui_element_create(d, DIVUI_ELEMENT_DIV, &e) == DIVUI_OK);
  property(e, DIVUI_STYLE_PROPERTY_WIDTH, w);
  property(e, DIVUI_STYLE_PROPERTY_HEIGHT, h);
  property(e, DIVUI_STYLE_PROPERTY_SCROLLBAR_WIDTH, "none");
  assert(divui_element_append_child(parent, e) == DIVUI_OK);
  return e;
}

static void offset(DivUiElement* e, double expected_x, double expected_y) {
  double x, y;
  assert(divui_element_get_scroll_offset(e, &x, &y) == DIVUI_OK);
  assert(x == expected_x && y == expected_y);
}

typedef struct Reveal {
  unsigned calls;
} Reveal;
static void clicked(DivUiEvent* event, void* user_data) {
  Reveal* state = (Reveal*)user_data;
  assert(event->event_type == DIVUI_EVENT_CLICK);
  assert(divui_element_scroll_into_view_v1(event->target, DIVUI_SCROLL_NEAREST,
                                           DIVUI_SCROLL_NEAREST,
                                           DIVUI_SCROLL_CONTAINERS_ALL) == DIVUI_OK);
  ++state->calls;
}

static void run(double scale, const char* overflow, double final_y) {
  DivUiDocumentConfig config = {sizeof(config),
                                DIVUI_ABI_VERSION,
                                {320.0, 240.0, (uint32_t)(320.0 * scale), (uint32_t)(240.0 * scale),
                                 scale, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* d = NULL;
  DivUiElement* root = NULL;
  assert(divui_document_create(&config, &d) == DIVUI_OK);
  assert(divui_document_root(d, &root) == DIVUI_OK);
  property(root, DIVUI_STYLE_PROPERTY_SCROLLBAR_WIDTH, "none");
  DivUiElement* outer = block(d, root, "100px", "80px");
  property(outer, DIVUI_STYLE_PROPERTY_POSITION, "absolute");
  property(outer, DIVUI_STYLE_PROPERTY_LEFT, "40px");
  property(outer, DIVUI_STYLE_PROPERTY_TOP, "30px");
  property(outer, DIVUI_STYLE_PROPERTY_OVERFLOW, "hidden");
  DivUiElement* spacer = block(d, outer, "200px", "200px");
  DivUiElement* inner = block(d, outer, "200px", "100px");
  property(inner, DIVUI_STYLE_PROPERTY_OVERFLOW, overflow);
  DivUiElement* target = block(d, inner, "20px", "20px");
  property(target, DIVUI_STYLE_PROPERTY_MARGIN_LEFT, "150px");
  property(target, DIVUI_STYLE_PROPERTY_MARGIN_TOP, "120px");
  assert(divui_element_scroll_into_view_v1(target, 99, DIVUI_SCROLL_NEAREST,
                                           DIVUI_SCROLL_CONTAINERS_ALL) ==
         DIVUI_ERROR_INVALID_ARGUMENT);
  assert(divui_element_smooth_scroll_into_view_v1(target, DIVUI_SCROLL_NEAREST,
                                                  DIVUI_SCROLL_NEAREST, DIVUI_SCROLL_CONTAINERS_ALL,
                                                  NAN) == DIVUI_ERROR_INVALID_ARGUMENT);
  offset(outer, 0, 0);
  Reveal state = {0};
  DivUiListener* listener = NULL;
  assert(divui_element_add_event_listener(target, DIVUI_EVENT_CLICK, 0, clicked, &state,
                                          &listener) == DIVUI_OK);
  DivUiEvent event;
  memset(&event, 0, sizeof(event));
  event.struct_size = sizeof(event);
  event.abi_version = DIVUI_ABI_VERSION;
  event.event_type = DIVUI_EVENT_CLICK;
  assert(divui_document_dispatch_event(d, target, &event) == DIVUI_OK);
  assert(state.calls == 1);
  offset(outer, 70, final_y);
  offset(inner, 0, strcmp(overflow, "clip") == 0 ? 0 : 40);
  DivUiRect bounds;
  assert(divui_element_get_bounds(target, &bounds) == DIVUI_OK);
  assert(bounds.x == 120 && bounds.y == 90 && bounds.width == 20 && bounds.height == 20);
  assert(divui_element_scroll_to(outer, 0, 0) == DIVUI_OK);
  assert(divui_element_scroll_to(inner, 0, 0) == DIVUI_OK);
  assert(divui_element_smooth_scroll_into_view_v1(target, DIVUI_SCROLL_NEAREST,
                                                  DIVUI_SCROLL_NEAREST, DIVUI_SCROLL_CONTAINERS_ALL,
                                                  100) == DIVUI_OK);
  offset(outer, 0, 0);
  assert(divui_document_set_animation_time(d, 100) == DIVUI_OK);
  offset(outer, 70, final_y);
  assert(divui_listener_destroy(listener) == DIVUI_OK);
  assert(divui_element_destroy(target) == DIVUI_OK);
  assert(divui_element_destroy(inner) == DIVUI_OK);
  assert(divui_element_destroy(spacer) == DIVUI_OK);
  assert(divui_element_destroy(outer) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(d) == DIVUI_OK);
}

int main(void) {
  const double scales[] = {1, 1.25, 1.5, 2, 3};
  for (unsigned i = 0; i < sizeof(scales) / sizeof(scales[0]); ++i) {
    run(scales[i], "hidden", 220);
    run(scales[i], "clip", 260);
  }
  return 0;
}
