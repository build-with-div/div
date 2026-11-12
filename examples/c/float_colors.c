#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <math.h>
#include <stdio.h>
#include <string.h>

#include "divui.h"

static void property(DivUiElement* element, DivUiStyleProperty id, const char* literal) {
  DivUiUtf8 text = {(const uint8_t*)literal, strlen(literal)};
  DivUiStyleValue value;
  assert(divui_style_value_parse(id, text, &value) == DIVUI_OK);
  assert(divui_element_set_property(element, id, &value) == DIVUI_OK);
  if (value.tag == DIVUI_STYLE_VALUE_COMPOUND)
    assert(divui_style_compound_destroy((DivUiStyleCompound*)value.data.compound) == DIVUI_OK);
}

static void color(DivUiElement* element,
                  DivUiStyleProperty id,
                  float red,
                  float green,
                  float blue,
                  float alpha) {
  DivUiStyleValue value;
  assert(divui_style_value_color_f32_v1(id, red, green, blue, alpha, &value) == DIVUI_OK);
  assert(value.tag == DIVUI_STYLE_VALUE_COMPOUND && value.reserved == 0);
  assert(divui_element_set_property(element, id, &value) == DIVUI_OK);
  assert(divui_element_set_property(element, DIVUI_STYLE_PROPERTY_WIDTH, &value) ==
         DIVUI_ERROR_WRONG_VALUE_TYPE);
  assert(divui_style_compound_destroy((DivUiStyleCompound*)value.data.compound) == DIVUI_OK);
  assert(divui_element_set_property(element, id, &value) == DIVUI_ERROR_INVALID_HANDLE);
}

typedef struct CallbackState {
  DivUiElement* card;
  unsigned calls;
} CallbackState;

static void clicked(DivUiEvent* event, void* data) {
  CallbackState* state = (CallbackState*)data;
  assert(event->event_type == DIVUI_EVENT_CLICK);
  color(state->card, DIVUI_STYLE_PROPERTY_BACKGROUND_COLOR, 0.0f, 0.0f, 1.0f, 0.5f);
  ++state->calls;
}

static void native_colors(double scale) {
  DivUiDocumentConfig config = {
      sizeof(config),
      DIVUI_ABI_VERSION,
      {64, 64, (uint32_t)(64 * scale), (uint32_t)(64 * scale), scale, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  DivUiElement *root = NULL, *card = NULL;
  assert(divui_document_create(&config, &document) == DIVUI_OK);
  assert(divui_document_root(document, &root) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &card) == DIVUI_OK);
  assert(divui_element_append_child(root, card) == DIVUI_OK);
  property(root, DIVUI_STYLE_PROPERTY_BACKGROUND_COLOR, "black");
  property(root, DIVUI_STYLE_PROPERTY_OVERFLOW, "hidden");
  property(card, DIVUI_STYLE_PROPERTY_WIDTH, "32px");
  property(card, DIVUI_STYLE_PROPERTY_HEIGHT, "32px");
  const DivUiStyleProperty properties[] = {
      DIVUI_STYLE_PROPERTY_BACKGROUND_COLOR,      DIVUI_STYLE_PROPERTY_COLOR,
      DIVUI_STYLE_PROPERTY_BORDER_TOP_COLOR,      DIVUI_STYLE_PROPERTY_BORDER_RIGHT_COLOR,
      DIVUI_STYLE_PROPERTY_BORDER_BOTTOM_COLOR,   DIVUI_STYLE_PROPERTY_BORDER_LEFT_COLOR,
      DIVUI_STYLE_PROPERTY_COLUMN_RULE_COLOR,     DIVUI_STYLE_PROPERTY_OUTLINE_COLOR,
      DIVUI_STYLE_PROPERTY_SCROLLBAR_TRACK_COLOR, DIVUI_STYLE_PROPERTY_SCROLLBAR_THUMB_COLOR,
      DIVUI_STYLE_PROPERTY_TEXT_DECORATION_COLOR, DIVUI_STYLE_PROPERTY_TEXT_EMPHASIS_COLOR};
  for (size_t i = 0; i < sizeof(properties) / sizeof(properties[0]); ++i)
    color(card, properties[i], 0.123456f, 0.234567f, 0.345678f, 0.5f);
  DivUiStyleValue output;
  memset(&output, 0, sizeof(output));
  output.tag = 77;
  output.reserved = 42;
  output.data.integer = 123;
  assert(divui_style_value_color_f32_v1(DIVUI_STYLE_PROPERTY_COLOR, NAN, 0, 0, 1, &output) ==
         DIVUI_ERROR_INVALID_ARGUMENT);
  assert(output.tag == 77 && output.reserved == 42 && output.data.integer == 123);
  color(card, DIVUI_STYLE_PROPERTY_BACKGROUND_COLOR, 1.0f, 0.0f, 0.0f, 0.5f);
  DivUiBitmap before = {sizeof(before), DIVUI_ABI_VERSION, 0, 0, 0, NULL};
  assert(divui_document_render_rgba(document, &before) == DIVUI_OK);
  CallbackState state = {card, 0};
  DivUiListener* listener = NULL;
  assert(divui_element_add_event_listener(card, DIVUI_EVENT_CLICK, 0, clicked, &state, &listener) ==
         DIVUI_OK);
  DivUiEvent event;
  memset(&event, 0, sizeof(event));
  event.struct_size = sizeof(event);
  event.abi_version = DIVUI_ABI_VERSION;
  event.event_type = DIVUI_EVENT_CLICK;
  assert(divui_document_dispatch_event(document, card, &event) == DIVUI_OK);
  assert(state.calls == 1);
  DivUiBitmap after = {sizeof(after), DIVUI_ABI_VERSION, 0, 0, 0, NULL};
  assert(divui_document_render_rgba(document, &after) == DIVUI_OK);
  const size_t sample = (size_t)(16 * scale) * before.stride + (size_t)(16 * scale) * 4;
  const uint8_t* initial = divui_buffer_data(before.pixels);
  const uint8_t* changed = divui_buffer_data(after.pixels);
  assert(initial[sample] == 128 && initial[sample + 1] == 0 && initial[sample + 2] == 0 &&
         initial[sample + 3] == 255);
  assert(changed[sample] == 0 && changed[sample + 1] == 0 && changed[sample + 2] == 128 &&
         changed[sample + 3] == 255);
  assert(divui_listener_destroy(listener) == DIVUI_OK);
  assert(divui_element_destroy(card) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(document) == DIVUI_OK);
  assert(divui_buffer_data(before.pixels)[sample] == 128);
  assert(divui_buffer_data(after.pixels)[sample + 2] == 128);
  assert(divui_buffer_destroy(before.pixels) == DIVUI_OK);
  assert(divui_buffer_destroy(after.pixels) == DIVUI_OK);
  printf("native float colors: scale=%g properties=12 callback=1 owned-frames=2 passed\n", scale);
}

int main(void) {
  const double scales[] = {1.0, 1.25, 1.5, 2.0, 3.0};
  for (size_t i = 0; i < sizeof(scales) / sizeof(scales[0]); ++i)
    native_colors(scales[i]);
  return 0;
}
