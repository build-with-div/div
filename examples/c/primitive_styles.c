/* Keep application checks active in release SDK builds. */
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "divui.h"

static DivUiUtf8 utf8(const char* value) {
  DivUiUtf8 result = {(const uint8_t*)value, strlen(value)};
  return result;
}

static void property(DivUiElement* element, DivUiStyleProperty id, const char* literal) {
  DivUiStyleValue value;
  DivUiStatus status = divui_style_value_parse(id, utf8(literal), &value);
  if (status != DIVUI_OK)
    fprintf(stderr, "native property %d literal '%s': %d\n", id, literal, status);
  assert(status == DIVUI_OK);
  assert(divui_element_set_property(element, id, &value) == DIVUI_OK);
  if (value.tag == DIVUI_STYLE_VALUE_COMPOUND)
    assert(divui_style_compound_destroy((DivUiStyleCompound*)value.data.compound) == DIVUI_OK);
}

typedef struct CallbackState {
  DivUiElement* card;
  unsigned calls;
} CallbackState;

static void clicked(DivUiEvent* event, void* data) {
  CallbackState* state = (CallbackState*)data;
  assert(event->event_type == DIVUI_EVENT_CLICK);
  property(state->card, DIVUI_STYLE_PROPERTY_BACKGROUND_COLOR, "blue");
  property(state->card, DIVUI_STYLE_PROPERTY_LEFT, "4em");
  property(state->card, DIVUI_STYLE_PROPERTY_FILTER_BLUR, "0");
  property(state->card, DIVUI_STYLE_PROPERTY_FILTER_GRAYSCALE, "0");
  property(state->card, DIVUI_STYLE_PROPERTY_COLUMN_WIDTH, "auto");
  property(state->card, DIVUI_STYLE_PROPERTY_SCROLLBAR_TRACK_COLOR, "green");
  ++state->calls;
}

static void native_styles(double scale) {
  DivUiDocumentConfig config = {sizeof(config),
                                DIVUI_ABI_VERSION,
                                {320.0, 240.0, (uint32_t)(320.0 * scale), (uint32_t)(240.0 * scale),
                                 scale, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  DivUiElement* root = NULL;
  DivUiElement* card = NULL;
  assert(divui_document_create(&config, &document) == DIVUI_OK);
  assert(divui_document_root(document, &root) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &card) == DIVUI_OK);
  assert(divui_element_append_child(root, card) == DIVUI_OK);
  property(root, DIVUI_STYLE_PROPERTY_OVERFLOW, "hidden");
  property(root, DIVUI_STYLE_PROPERTY_BACKGROUND_COLOR, "white");
  property(card, DIVUI_STYLE_PROPERTY_POSITION, "absolute");
  property(card, DIVUI_STYLE_PROPERTY_DISPLAY, "block");
  property(card, DIVUI_STYLE_PROPERTY_WIDTH, "80px");
  property(card, DIVUI_STYLE_PROPERTY_HEIGHT, "40px");
  property(card, DIVUI_STYLE_PROPERTY_FONT_SIZE, "20px");
  property(card, DIVUI_STYLE_PROPERTY_COLOR, "green");
  property(card, DIVUI_STYLE_PROPERTY_BACKGROUND_COLOR, "red");
  property(card, DIVUI_STYLE_PROPERTY_BORDER, "1px solid black");

  const struct {
    DivUiStyleProperty id;
    const char* literal;
  } values[] = {
      {DIVUI_STYLE_PROPERTY_ALIGN_CONTENT, "safe end"},
      {DIVUI_STYLE_PROPERTY_ALIGN_SELF, "unsafe center"},
      {DIVUI_STYLE_PROPERTY_BORDER_BOTTOM_COLOR, "currentcolor"},
      {DIVUI_STYLE_PROPERTY_BORDER_BOTTOM_WIDTH, "1"},
      {DIVUI_STYLE_PROPERTY_BORDER_LEFT_COLOR, "#123456"},
      {DIVUI_STYLE_PROPERTY_BORDER_LEFT_WIDTH, "2"},
      {DIVUI_STYLE_PROPERTY_BORDER_RIGHT_COLOR, "blue"},
      {DIVUI_STYLE_PROPERTY_BORDER_RIGHT_WIDTH, "3"},
      {DIVUI_STYLE_PROPERTY_BORDER_TOP_COLOR, "red"},
      {DIVUI_STYLE_PROPERTY_BORDER_TOP_WIDTH, "4"},
      {DIVUI_STYLE_PROPERTY_BOTTOM, "auto"},
      {DIVUI_STYLE_PROPERTY_COLUMN_HEIGHT, "2em"},
      {DIVUI_STYLE_PROPERTY_COLUMN_RULE_COLOR, "currentcolor"},
      {DIVUI_STYLE_PROPERTY_COLUMN_RULE_WIDTH, "2"},
      {DIVUI_STYLE_PROPERTY_COLUMN_WIDTH, "96px"},
      {DIVUI_STYLE_PROPERTY_FILTER_BLUR, "1.25"},
      {DIVUI_STYLE_PROPERTY_FILTER_GRAYSCALE, "0.4"},
      {DIVUI_STYLE_PROPERTY_JUSTIFY_ITEMS, "self-end"},
      {DIVUI_STYLE_PROPERTY_JUSTIFY_SELF, "last baseline"},
      {DIVUI_STYLE_PROPERTY_LEFT, "2em"},
      {DIVUI_STYLE_PROPERTY_ORDER, "-7"},
      {DIVUI_STYLE_PROPERTY_ORPHANS, "4294967295"},
      {DIVUI_STYLE_PROPERTY_OUTLINE_COLOR, "currentcolor"},
      {DIVUI_STYLE_PROPERTY_OUTLINE_OFFSET, "-2"},
      {DIVUI_STYLE_PROPERTY_OVERFLOW_CLIP_MARGIN, "2.5"},
      {DIVUI_STYLE_PROPERTY_OVERFLOW_X, "clip"},
      {DIVUI_STYLE_PROPERTY_OVERFLOW_Y, "hidden"},
      {DIVUI_STYLE_PROPERTY_RIGHT, "auto"},
      {DIVUI_STYLE_PROPERTY_SCROLLBAR_TRACK_COLOR, "auto"},
      {DIVUI_STYLE_PROPERTY_TOP, "10vh"},
      {DIVUI_STYLE_PROPERTY_WIDOWS, "3"},
      {DIVUI_STYLE_PROPERTY_OUTLINE_WIDTH, "2"},
      {DIVUI_STYLE_PROPERTY_SCROLLBAR_THUMB_COLOR, "#123456"},
      {DIVUI_STYLE_PROPERTY_SHAPE_IMAGE_THRESHOLD, "0.25"},
      {DIVUI_STYLE_PROPERTY_SHAPE_MARGIN, "1.5rem"},
  };
  for (size_t i = 0; i < sizeof(values) / sizeof(values[0]); ++i)
    property(card, values[i].id, values[i].literal);

  DivUiRect original;
  assert(divui_element_get_bounds(card, &original) == DIVUI_OK);
  assert(original.x == 40.0f && original.y == 24.0f);
  assert(original.width == 85.0f && original.height == 45.0f);
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
  DivUiRect changed;
  assert(divui_element_get_bounds(card, &changed) == DIVUI_OK && changed.x == 80.0f);
  assert(changed.y == 24.0f && changed.width == 85.0f && changed.height == 45.0f);
  DivUiBitmap after = {sizeof(after), DIVUI_ABI_VERSION, 0, 0, 0, NULL};
  assert(divui_document_render_rgba(document, &after) == DIVUI_OK);
  assert(before.width == after.width && before.height == after.height);
  assert(divui_buffer_length(before.pixels) == divui_buffer_length(after.pixels));
  assert(memcmp(divui_buffer_data(before.pixels), divui_buffer_data(after.pixels),
                divui_buffer_length(before.pixels)) != 0);
  size_t sample = (size_t)(40.0 * scale) * after.stride + (size_t)(100.0 * scale) * 4;
  const uint8_t* pixels = divui_buffer_data(after.pixels);
  if (pixels[sample] != 0 || pixels[sample + 1] != 0 || pixels[sample + 2] != 255 ||
      pixels[sample + 3] != 255)
    fprintf(stderr, "native blue pixel at scale=%g: %u,%u,%u,%u\n", scale, pixels[sample],
            pixels[sample + 1], pixels[sample + 2], pixels[sample + 3]);
  assert(pixels[sample] == 0 && pixels[sample + 1] == 0 && pixels[sample + 2] == 255 &&
         pixels[sample + 3] == 255);
  assert(original.x == 40.0f && original.width == 85.0f);
  assert(divui_listener_destroy(listener) == DIVUI_OK);
  assert(divui_element_destroy(card) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(document) == DIVUI_OK);
  /* Owned render buffers outlive the document. */
  assert(divui_buffer_data(after.pixels)[sample + 2] == 255);
  assert(divui_buffer_destroy(before.pixels) == DIVUI_OK);
  assert(divui_buffer_destroy(after.pixels) == DIVUI_OK);
  printf("native C primitive styles: scale=%g properties=35 callback=1 passed\n", scale);
}

int main(void) {
  const double scales[] = {1.0, 1.25, 1.5, 2.0, 3.0};
  for (size_t i = 0; i < sizeof(scales) / sizeof(scales[0]); ++i)
    native_styles(scales[i]);
  return 0;
}
