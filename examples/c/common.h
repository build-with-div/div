#ifndef DIV_EXAMPLE_COMMON_H_
#define DIV_EXAMPLE_COMMON_H_

#include <stdio.h>
#include <string.h>

#include "divui.h"

static DivUiUtf8 divui_example_utf8(const char* value) {
  DivUiUtf8 result = {(const uint8_t*)value, strlen(value)};
  return result;
}

static int divui_example_check(DivUiStatus status, const char* operation) {
  if (status == DIVUI_OK)
    return 1;
  DivUiErrorInfo error = {sizeof(error), DIVUI_ABI_VERSION, 0, 0, 0};
  divui_error_get_last(&error);
  fprintf(stderr, "%s failed (%d, detail %u)\n", operation, error.status, error.detail);
  return 0;
}

static DivUiStyleValue divui_example_px(float value) {
  DivUiStyleValue result = {.tag = DIVUI_STYLE_VALUE_LENGTH, .reserved = 0, .data = {.integer = 0}};
  result.data.length.value = value;
  result.data.length.unit = DIVUI_LENGTH_PX;
  return result;
}

static int divui_example_render(const char* label, DivUiColor background) {
  DivUiDocumentConfig config = {
      sizeof(config), DIVUI_ABI_VERSION, {480.0, 240.0, 480, 240, 1.0, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  DivUiElement* root = NULL;
  DivUiElement* card = NULL;
  DivUiElement* found = NULL;
  DivUiElement* text_node = NULL;
  DivUiBuffer* png = NULL;
  int result = 1;
  if (!divui_example_check(divui_document_create(&config, &document), "create document") ||
      !divui_example_check(divui_document_root(document, &root), "get root") ||
      !divui_example_check(divui_element_create(document, DIVUI_ELEMENT_DIV, &card),
                           "create card") ||
      !divui_example_check(divui_text_create(document, divui_example_utf8(label), &text_node),
                           "create text") ||
      !divui_example_check(divui_element_append_child(root, card), "append card") ||
      !divui_example_check(divui_element_append_child(card, text_node), "append text") ||
      !divui_example_check(divui_element_set_attribute(card, divui_example_utf8("id"),
                                                       divui_example_utf8("primary")),
                           "set ID") ||
      !divui_example_check(
          divui_document_element_by_id(document, divui_example_utf8("primary"), &found),
          "find card") ||
      found == NULL) {
    result = 0;
    goto cleanup;
  }
  DivUiStyleValue width = divui_example_px(440.0f);
  DivUiStyleValue height = divui_example_px(200.0f);
  DivUiStyleValue color = {.tag = DIVUI_STYLE_VALUE_COLOR, .reserved = 0, .data = {.integer = 0}};
  color.data.color = background;
  if (!divui_example_check(divui_element_set_property(card, DIVUI_STYLE_PROPERTY_WIDTH, &width),
                           "set width") ||
      !divui_example_check(divui_element_set_property(card, DIVUI_STYLE_PROPERTY_HEIGHT, &height),
                           "set height") ||
      !divui_example_check(
          divui_element_set_property(found, DIVUI_STYLE_PROPERTY_BACKGROUND_COLOR, &color),
          "set color") ||
      !divui_example_check(divui_document_render_png(document, &png), "render PNG")) {
    result = 0;
    goto cleanup;
  }
  printf("%s: %zu PNG bytes\n", label, divui_buffer_length(png));

cleanup:
  if (png)
    divui_buffer_destroy(png);
  if (found)
    divui_element_destroy(found);
  if (text_node)
    divui_element_destroy(text_node);
  if (card)
    divui_element_destroy(card);
  if (root)
    divui_element_destroy(root);
  if (document)
    divui_document_destroy(document);
  return result;
}

#endif
