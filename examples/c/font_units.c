/* Keep consumer checks active in packaged release builds. */
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "divui.h"

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

static DivUiStyleValue length(float number, DivUiLengthUnit unit) {
  DivUiStyleValue value;
  memset(&value, 0, sizeof(value));
  value.tag = DIVUI_STYLE_VALUE_LENGTH;
  value.data.length.value = number;
  value.data.length.unit = unit;
  return value;
}

typedef struct CallbackState {
  DivUiElement* parent;
  unsigned calls;
} CallbackState;

static void clicked(DivUiEvent* event, void* data) {
  CallbackState* state = (CallbackState*)data;
  assert(event->event_type == DIVUI_EVENT_CLICK);
  property(state->parent, DIVUI_STYLE_PROPERTY_FONT_SIZE, "32px");
  ++state->calls;
}

static void native_font_units(double scale) {
  DivUiDocumentConfig config = {sizeof(config),
                                DIVUI_ABI_VERSION,
                                {320, 240, (uint32_t)(320 * scale), (uint32_t)(240 * scale), scale,
                                 DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  DivUiElement *root = NULL, *nodes[3] = {NULL, NULL, NULL};
  assert(divui_document_create(&config, &document) == DIVUI_OK);
  assert(divui_document_root(document, &root) == DIVUI_OK);
  FILE* input = fopen("crates/div-text/fonts/Ahem.ttf", "rb");
  assert(input != NULL && fseek(input, 0, SEEK_END) == 0);
  const long count = ftell(input);
  assert(count > 0 && fseek(input, 0, SEEK_SET) == 0);
  uint8_t* bytes = (uint8_t*)malloc((size_t)count);
  assert(bytes != NULL && fread(bytes, 1, (size_t)count, input) == (size_t)count);
  assert(fclose(input) == 0);
  DivUiFontFaceDescriptor descriptor;
  memset(&descriptor, 0, sizeof(descriptor));
  descriptor.struct_size = sizeof(descriptor);
  descriptor.abi_version = DIVUI_ABI_VERSION;
  descriptor.family = text("Native unit Ahem");
  descriptor.weight_min = descriptor.weight_max = 400;
  descriptor.stretch_min = descriptor.stretch_max = 100;
  DivUiFontFace* face = NULL;
  assert(divui_document_register_font(document, bytes, (size_t)count, &descriptor, &face) ==
         DIVUI_OK);
  free(bytes);
  property(root, DIVUI_STYLE_PROPERTY_FONT_FAMILY, "Native unit Ahem");
  property(root, DIVUI_STYLE_PROPERTY_FONT_SIZE, "16px");
  property(root, DIVUI_STYLE_PROPERTY_LINE_HEIGHT, "1.5");
  const DivUiLengthUnit units[] = {DIVUI_LENGTH_CH, DIVUI_LENGTH_EX, DIVUI_LENGTH_LH};
  const float coefficients[] = {2.0f, 2.5f, 2.0f};
  const float before[] = {32.0f, 32.0f, 48.0f};
  const float after[] = {64.0f, 64.0f, 96.0f};
  DivUiRect owned[3];
  for (size_t i = 0; i < 3; ++i) {
    assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &nodes[i]) == DIVUI_OK);
    assert(divui_element_append_child(root, nodes[i]) == DIVUI_OK);
    DivUiStyleValue width = length(coefficients[i], units[i]);
    assert(divui_element_set_property(nodes[i], DIVUI_STYLE_PROPERTY_WIDTH, &width) == DIVUI_OK);
    property(nodes[i], DIVUI_STYLE_PROPERTY_HEIGHT, "10px");
    /* Typed length-valued line-height uses the same shared retained Engine. */
    assert(divui_element_set_property(nodes[i], DIVUI_STYLE_PROPERTY_LINE_HEIGHT, &width) ==
           DIVUI_OK);
    DivUiStyleValue dependent = length(2.5f, DIVUI_LENGTH_LH);
    assert(divui_element_set_property(nodes[i], DIVUI_STYLE_PROPERTY_WIDTH, &dependent) ==
           DIVUI_OK);
    assert(divui_element_get_bounds(nodes[i], &owned[i]) == DIVUI_OK);
    assert(owned[i].width == before[i] * 2.5f);
    width.data.length.value = NAN;
    assert(divui_element_set_property(nodes[i], DIVUI_STYLE_PROPERTY_WIDTH, &width) ==
           DIVUI_ERROR_INVALID_ARGUMENT);
  }
  CallbackState state = {root, 0};
  DivUiListener* listener = NULL;
  assert(divui_element_add_event_listener(nodes[0], DIVUI_EVENT_CLICK, 0, clicked, &state,
                                          &listener) == DIVUI_OK);
  DivUiEvent event;
  memset(&event, 0, sizeof(event));
  event.struct_size = sizeof(event);
  event.abi_version = DIVUI_ABI_VERSION;
  event.event_type = DIVUI_EVENT_CLICK;
  assert(divui_document_dispatch_event(document, nodes[0], &event) == DIVUI_OK);
  assert(state.calls == 1);
  for (size_t i = 0; i < 3; ++i) {
    DivUiRect changed;
    assert(divui_element_get_bounds(nodes[i], &changed) == DIVUI_OK);
    assert(changed.width == after[i] * 2.5f && owned[i].width == before[i] * 2.5f);
    assert(divui_element_destroy(nodes[i]) == DIVUI_OK);
  }
  assert(divui_listener_destroy(listener) == DIVUI_OK);
  assert(divui_font_face_destroy(face) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(document) == DIVUI_OK);
  printf("native retained font units: scale=%g units=3 callback=1 passed\n", scale);
}

int main(void) {
  const double scales[] = {1.0, 1.25, 1.5, 2.0, 3.0};
  for (size_t i = 0; i < sizeof(scales) / sizeof(scales[0]); ++i)
    native_font_units(scales[i]);
  return 0;
}
