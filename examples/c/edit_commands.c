/* Keep consumer checks active when the SDK is built with NDEBUG. */
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "divui.h"

static DivUiUtf8 text(const char* value) {
  DivUiUtf8 result = {(const uint8_t*)value, strlen(value)};
  return result;
}

static DivUiEditCommandV1 command(uint32_t kind) {
  DivUiEditCommandV1 result = {sizeof(result), DIVUI_ABI_VERSION, kind, 0, 0, 0, {0, 0}};
  return result;
}

static void value_is(DivUiElement* element, const char* expected) {
  uint8_t bytes[64];
  size_t length = 0;
  assert(divui_element_copy_control_value(element, bytes, sizeof(bytes), &length) == DIVUI_OK);
  assert(length == strlen(expected) && memcmp(bytes, expected, length) == 0);
}

static void selection_is(DivUiElement* element, size_t start, size_t end) {
  size_t actual_start = 0;
  size_t actual_end = 0;
  assert(divui_element_get_selection(element, &actual_start, &actual_end) == DIVUI_OK);
  assert(actual_start == start && actual_end == end);
}

typedef struct Context {
  DivUiElement* input;
  DivUiElement* sibling;
  unsigned input_events;
  unsigned click_events;
} Context;

static void on_input(DivUiEvent* event, void* user_data) {
  Context* context = (Context*)user_data;
  assert(event->event_type == DIVUI_EVENT_INPUT);
  assert(event->target == context->input && event->current_target == context->input);
  ++context->input_events;
  value_is(context->input, context->input_events == 2 ? "á👩‍💻z" : "á👩‍💻");
  /* Reenter the retained document while an input callback is running. */
  assert(divui_element_set_text(context->sibling, text("native input callback")) == DIVUI_OK);
}

static void on_click(DivUiEvent* event, void* user_data) {
  Context* context = (Context*)user_data;
  assert(event->event_type == DIVUI_EVENT_CLICK);
  ++context->click_events;
  DivUiEditCommandV1 edit = command(DIVUI_EDIT_DELETE);
  assert(divui_element_edit_text_v1(context->input, &edit) == DIVUI_OK);
}

static void exercise(uint32_t kind) {
  DivUiDocumentConfig config = {
      sizeof(config), DIVUI_ABI_VERSION, {320.0, 200.0, 320, 200, 1.0, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  DivUiElement* root = NULL;
  DivUiElement* button = NULL;
  DivUiListener* input_listener = NULL;
  DivUiListener* click_listener = NULL;
  Context context = {NULL, NULL, 0, 0};
  assert(divui_document_create(&config, &document) == DIVUI_OK);
  assert(divui_document_root(document, &root) == DIVUI_OK);
  assert(divui_element_create(document, (DivUiElementTag)kind, &context.input) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &context.sibling) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_BUTTON, &button) == DIVUI_OK);
  assert(divui_element_append_child(root, context.input) == DIVUI_OK);
  assert(divui_element_append_child(root, context.sibling) == DIVUI_OK);
  assert(divui_element_append_child(root, button) == DIVUI_OK);
  assert(divui_element_set_control_value(context.input, text("á👩‍💻z")) == DIVUI_OK);
  assert(divui_element_set_selection(context.input, 15, 15) == DIVUI_OK);
  assert(divui_element_add_event_listener(context.input, DIVUI_EVENT_INPUT, 0, on_input, &context,
                                          &input_listener) == DIVUI_OK);
  assert(divui_element_add_event_listener(button, DIVUI_EVENT_CLICK, 0, on_click, &context,
                                          &click_listener) == DIVUI_OK);
  DivUiEvent click = {sizeof(click),
                      DIVUI_ABI_VERSION,
                      DIVUI_EVENT_CLICK,
                      0,
                      0,
                      0,
                      0,
                      0,
                      0,
                      0,
                      0,
                      0,
                      0,
                      {NULL, 0},
                      NULL,
                      NULL};
  assert(divui_document_dispatch_event(document, button, &click) == DIVUI_OK);
  DivUiEditCommandV1 edit = command(DIVUI_EDIT_UNDO);
  assert(divui_element_edit_text_v1(context.input, &edit) == DIVUI_OK);
  edit = command(DIVUI_EDIT_REDO);
  assert(divui_element_edit_text_v1(context.input, &edit) == DIVUI_OK);
  assert(context.input_events == 3 && context.click_events == 1);
  value_is(context.input, "á👩‍💻");

  edit = command(DIVUI_EDIT_MOVE);
  assert(divui_element_edit_text_v1(context.input, &edit) == DIVUI_OK);
  selection_is(context.input, 3, 3);
  edit.extend_selection = 1;
  assert(divui_element_edit_text_v1(context.input, &edit) == DIVUI_OK);
  selection_is(context.input, 0, 3);
  edit = command(DIVUI_EDIT_SELECT_ALL);
  assert(divui_element_edit_text_v1(context.input, &edit) == DIVUI_OK);
  selection_is(context.input, 0, 14);
  edit = command(DIVUI_EDIT_MOVE);
  edit.direction = DIVUI_TEXT_FORWARD;
  edit.unit = DIVUI_TEXT_DOCUMENT;
  assert(divui_element_edit_text_v1(context.input, &edit) == DIVUI_OK);
  selection_is(context.input, 14, 14);

  assert(divui_element_set_control_value(context.input, text("one two three")) == DIVUI_OK);
  assert(divui_element_set_selection(context.input, 13, 13) == DIVUI_OK);
  edit.direction = DIVUI_TEXT_BACKWARD;
  edit.unit = DIVUI_TEXT_WORD;
  assert(divui_element_edit_text_v1(context.input, &edit) == DIVUI_OK);
  selection_is(context.input, 8, 8);
  const char* lines = kind == DIVUI_ELEMENT_TEXTAREA ? "one\ntwo" : "one two";
  assert(divui_element_set_control_value(context.input, text(lines)) == DIVUI_OK);
  assert(divui_element_set_selection(context.input, 7, 7) == DIVUI_OK);
  edit.unit = DIVUI_TEXT_LINE;
  assert(divui_element_edit_text_v1(context.input, &edit) == DIVUI_OK);
  size_t line_start = kind == DIVUI_ELEMENT_TEXTAREA ? 4 : 0;
  selection_is(context.input, line_start, line_start);
  assert(context.input_events == 3);
  assert(divui_element_set_attribute(context.input, text("readonly"), text("")) == DIVUI_OK);
  edit = command(DIVUI_EDIT_SELECT_ALL);
  assert(divui_element_edit_text_v1(context.input, &edit) == DIVUI_OK);
  edit = command(DIVUI_EDIT_DELETE);
  assert(divui_element_edit_text_v1(context.input, &edit) == DIVUI_ERROR_INVALID_STATE);
  edit.reserved[1] = 1;
  assert(divui_element_edit_text_v1(context.input, &edit) == DIVUI_ERROR_INVALID_ARGUMENT);
  edit = command(DIVUI_EDIT_SELECT_ALL);
  assert(divui_element_set_attribute(context.input, text("disabled"), text("")) == DIVUI_OK);
  assert(divui_element_edit_text_v1(context.input, &edit) == DIVUI_ERROR_INVALID_STATE);
  assert(divui_listener_destroy(input_listener) == DIVUI_OK);
  assert(divui_listener_destroy(click_listener) == DIVUI_OK);
  assert(divui_element_remove(context.input) == DIVUI_OK);
  assert(divui_element_edit_text_v1(context.input, &edit) == DIVUI_ERROR_STALE_HANDLE);
  assert(divui_element_destroy(context.input) == DIVUI_OK);
  assert(divui_element_edit_text_v1(context.input, &edit) == DIVUI_ERROR_INVALID_HANDLE);
  assert(divui_element_destroy(button) == DIVUI_OK);
  assert(divui_element_destroy(context.sibling) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(document) == DIVUI_OK);
  printf(
      "native editing: kind=%u input=%u click=%u unicode, history, selection and teardown passed\n",
      kind, context.input_events, context.click_events);
}

int main(void) {
  exercise(DIVUI_ELEMENT_INPUT);
  exercise(DIVUI_ELEMENT_TEXTAREA);
  return 0;
}
