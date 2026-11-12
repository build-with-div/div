/* Native text replacement through the shared Rust engine and C callbacks. */
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

static void length(DivUiElement* element, DivUiStyleProperty property, float pixels) {
  DivUiStyleValue value;
  memset(&value, 0, sizeof(value));
  value.tag = DIVUI_STYLE_VALUE_LENGTH;
  value.data.length.value = pixels;
  value.data.length.unit = DIVUI_LENGTH_PX;
  assert(divui_element_set_property(element, property, &value) == DIVUI_OK);
}

static void color(DivUiElement* element,
                  DivUiStyleProperty property,
                  uint8_t red,
                  uint8_t green,
                  uint8_t blue) {
  DivUiStyleValue value;
  memset(&value, 0, sizeof(value));
  value.tag = DIVUI_STYLE_VALUE_COLOR;
  value.data.color.red = red;
  value.data.color.green = green;
  value.data.color.blue = blue;
  value.data.color.alpha = 255;
  assert(divui_element_set_property(element, property, &value) == DIVUI_OK);
}

static void enumeration(DivUiElement* element, DivUiStyleProperty property, int32_t number) {
  DivUiStyleValue value;
  memset(&value, 0, sizeof(value));
  value.tag = DIVUI_STYLE_VALUE_ENUM;
  value.data.enum_value = number;
  assert(divui_element_set_property(element, property, &value) == DIVUI_OK);
}

typedef struct Mutation {
  DivUiElement** elements;
  size_t count;
  unsigned calls;
  int glyphs;
} Mutation;

static void clicked(DivUiEvent* event, void* user_data) {
  Mutation* state = (Mutation*)user_data;
  assert(event->event_type == DIVUI_EVENT_CLICK);
  /* Query and mutation during a callback prove engine borrows were released. */
  for (size_t index = 0; index < state->count; ++index) {
    if (state->glyphs) {
      assert(divui_element_set_text(state->elements[index], text("XX")) == DIVUI_OK);
      color(state->elements[index], DIVUI_STYLE_PROPERTY_COLOR, 0, 0, 255);
    } else {
      color(state->elements[index], DIVUI_STYLE_PROPERTY_BACKGROUND_COLOR, 0, 0, 255);
    }
  }
  ++state->calls;
}

static void click(DivUiDocument* document, DivUiElement* target) {
  DivUiEvent event;
  memset(&event, 0, sizeof(event));
  event.struct_size = sizeof(event);
  event.abi_version = DIVUI_ABI_VERSION;
  event.event_type = DIVUI_EVENT_CLICK;
  assert(divui_document_dispatch_event(document, target, &event) == DIVUI_OK);
}

static DivUiFontFace* register_font(DivUiDocument* document, const char* family, const char* path) {
  FILE* input = fopen(path, "rb");
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
  descriptor.family = text(family);
  descriptor.weight_min = descriptor.weight_max = 400;
  descriptor.stretch_min = descriptor.stretch_max = 100;
  DivUiFontFace* face = NULL;
  assert(divui_document_register_font(document, bytes, (size_t)count, &descriptor, &face) ==
         DIVUI_OK);
  free(bytes);
  return face;
}

static void save_png(DivUiDocument* document, const char* folder, const char* state) {
  char path[4096];
  int count = snprintf(path, sizeof(path), "%s/%s.png", folder, state);
  assert(count > 0 && (size_t)count < sizeof(path));
  DivUiBuffer* png = NULL;
  assert(divui_document_render_png(document, &png) == DIVUI_OK);
  FILE* output = fopen(path, "wb");
  assert(output != NULL);
  const size_t length = divui_buffer_length(png);
  assert(fwrite(divui_buffer_data(png), 1, length, output) == length);
  assert(fclose(output) == 0);
  assert(divui_buffer_destroy(png) == DIVUI_OK);
}

static void write_bounds(FILE* output, DivUiElement** nodes) {
  fputc('[', output);
  for (size_t phase = 0; phase < 64; ++phase) {
    DivUiRect rect;
    assert(divui_element_get_bounds(nodes[phase], &rect) == DIVUI_OK);
    fprintf(output, "%s{\"x\":%.17g,\"y\":%.17g,\"width\":%.17g,\"height\":%.17g}",
            phase ? "," : "", (double)rect.x, (double)rect.y, (double)rect.width,
            (double)rect.height);
  }
  fputc(']', output);
}

static void font_consumer(const char* folder,
                          double scale,
                          const char* family,
                          float size,
                          const char* font_path) {
  assert(isfinite(scale) && scale > 0 && isfinite(size) && size > 0);
  DivUiDocumentConfig viewport = {
      sizeof(viewport), DIVUI_ABI_VERSION, {800, 600, 0, 0, scale, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  assert(divui_document_create(&viewport, &document) == DIVUI_OK);
  DivUiFontFace* face = register_font(document, family, font_path);
  DivUiStyleCompound* font_family = NULL;
  assert(divui_font_family_create(text(family), 0, &font_family) == DIVUI_OK);
  DivUiStyleValue font;
  memset(&font, 0, sizeof(font));
  font.tag = DIVUI_STYLE_VALUE_COMPOUND;
  font.data.compound = font_family;
  DivUiElement* root = NULL;
  assert(divui_document_root(document, &root) == DIVUI_OK);
  color(root, DIVUI_STYLE_PROPERTY_BACKGROUND_COLOR, 255, 255, 255);
  enumeration(root, DIVUI_STYLE_PROPERTY_OVERFLOW, DIVUI_OVERFLOW_HIDDEN);
  DivUiElement* nodes[64];
  for (size_t phase = 0; phase < 64; ++phase) {
    assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &nodes[phase]) == DIVUI_OK);
    enumeration(nodes[phase], DIVUI_STYLE_PROPERTY_POSITION, DIVUI_POSITION_ABSOLUTE);
    length(nodes[phase], DIVUI_STYLE_PROPERTY_LEFT,
           20 + (float)(phase % 8) * 92 + (float)phase / 64);
    length(nodes[phase], DIVUI_STYLE_PROPERTY_TOP, 20 + (float)(phase / 8) * 60);
    assert(divui_element_set_property(nodes[phase], DIVUI_STYLE_PROPERTY_FONT_FAMILY, &font) ==
           DIVUI_OK);
    length(nodes[phase], DIVUI_STYLE_PROPERTY_FONT_SIZE, size);
    DivUiStyleValue line_height;
    assert(divui_style_value_parse(DIVUI_STYLE_PROPERTY_LINE_HEIGHT, text("1"), &line_height) ==
           DIVUI_OK);
    assert(divui_element_set_property(nodes[phase], DIVUI_STYLE_PROPERTY_LINE_HEIGHT,
                                      &line_height) == DIVUI_OK);
    if (line_height.tag == DIVUI_STYLE_VALUE_COMPOUND)
      assert(divui_style_compound_destroy((DivUiStyleCompound*)line_height.data.compound) ==
             DIVUI_OK);
    color(nodes[phase], DIVUI_STYLE_PROPERTY_COLOR, 0, 0, 0);
    assert(divui_element_set_text(nodes[phase], text("X")) == DIVUI_OK);
    assert(divui_element_append_child(root, nodes[phase]) == DIVUI_OK);
  }
  assert(divui_style_compound_destroy(font_family) == DIVUI_OK);
  char path[4096];
  int count = snprintf(path, sizeof(path), "%s/geometry.json", folder);
  assert(count > 0 && (size_t)count < sizeof(path));
  FILE* output = fopen(path, "wb");
  assert(output != NULL);
  fputs("{\"before\":", output);
  write_bounds(output, nodes);
  save_png(document, folder, "before");
  Mutation mutation = {nodes, 64, 0, 1};
  DivUiListener* listener = NULL;
  assert(divui_element_add_event_listener(root, DIVUI_EVENT_CLICK, 0, clicked, &mutation,
                                          &listener) == DIVUI_OK);
  click(document, root);
  assert(mutation.calls == 1);
  fputs(",\"after\":", output);
  write_bounds(output, nodes);
  fprintf(output, ",\"callback_count\":%u}\n", mutation.calls);
  assert(fclose(output) == 0);
  save_png(document, folder, "after");
  assert(divui_listener_destroy(listener) == DIVUI_OK);
  for (size_t phase = 0; phase < 64; ++phase)
    assert(divui_element_destroy(nodes[phase]) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_font_face_destroy(face) == DIVUI_OK);
  assert(divui_document_destroy(document) == DIVUI_OK);
}

int main(int argc, char** argv) {
  if (argc == 1) {
    DivUiDocumentConfig config = {
        sizeof(config), DIVUI_ABI_VERSION, {80, 60, 0, 0, 1, DIVUI_VIEWPORT_LOGICAL, 0}};
    DivUiDocument* document = NULL;
    DivUiElement *root = NULL, *label = NULL;
    assert(divui_document_create(&config, &document) == DIVUI_OK);
    assert(divui_document_root(document, &root) == DIVUI_OK);
    assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &label) == DIVUI_OK);
    length(label, DIVUI_STYLE_PROPERTY_WIDTH, 40);
    length(label, DIVUI_STYLE_PROPERTY_HEIGHT, 20);
    assert(divui_element_set_text(label, text("X")) == DIVUI_OK);
    assert(divui_element_append_child(root, label) == DIVUI_OK);
    DivUiElement* nodes[] = {label};
    Mutation mutation = {nodes, 1, 0, 1};
    DivUiListener* listener = NULL;
    assert(divui_element_add_event_listener(root, DIVUI_EVENT_CLICK, 0, clicked, &mutation,
                                            &listener) == DIVUI_OK);
    click(document, root);
    assert(mutation.calls == 1);
    DivUiRect bounds;
    assert(divui_element_get_bounds(label, &bounds) == DIVUI_OK);
    assert(bounds.width == 40 && bounds.height == 20);
    assert(divui_listener_destroy(listener) == DIVUI_OK);
    assert(divui_element_destroy(label) == DIVUI_OK);
    assert(divui_element_destroy(root) == DIVUI_OK);
    assert(divui_document_destroy(document) == DIVUI_OK);
    puts("native text replacement: consuming C callback and owned bounds passed");
    return 0;
  }
  if (argc != 6)
    return 2;
  font_consumer(argv[1], strtod(argv[2], NULL), argv[3], strtof(argv[4], NULL), argv[5]);
  return 0;
}
