/* Native C/C++ consumer: callback cancellation and completed control state. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "divui.h"

#define CHECK(call)                          \
  do {                                       \
    if ((call) != DIVUI_OK) {                \
      fprintf(stderr, "%s failed\n", #call); \
      exit(1);                               \
    }                                        \
  } while (0)
typedef struct CallbackData {
  const char* target;
  int modifier;
} CallbackData;
static size_t rows;
static DivUiUtf8 text(const char* value) {
  DivUiUtf8 result = {(const uint8_t*)value, strlen(value)};
  return result;
}
static const char* kind(uint32_t type) {
  switch (type) {
    case DIVUI_EVENT_BEFORE_INPUT:
      return "beforeinput";
    case DIVUI_EVENT_INPUT:
      return "input";
    case DIVUI_EVENT_CHANGE:
      return "change";
    default:
      exit(1);
  }
}
static void callback(DivUiEvent* event, void* user_data) {
  CallbackData* data = (CallbackData*)user_data;
  if (data->modifier) {
    if (event->event_type != DIVUI_EVENT_BEFORE_INPUT ||
        (event->text.length == 7 && memcmp(event->text.data, "blocked", 7) == 0)) {
      event->flags |= DIVUI_EVENT_DEFAULT_PREVENTED;
    }
    return;
  }
  if (rows++)
    printf(",");
  printf("{\"type\":\"%s\",\"target\":\"%s\",\"after\":%s}", kind(event->event_type), data->target,
         event->flags & DIVUI_EVENT_DEFAULT_PREVENTED ? "true" : "false");
}
int main(void) {
  DivUiDocument* document = NULL;
  DivUiElement* root = NULL;
  DivUiElement* input = NULL;
  DivUiElement* box = NULL;
  DivUiDocumentConfig config = {
      sizeof(config), DIVUI_ABI_VERSION, {320, 200, 320, 200, 1.0, DIVUI_VIEWPORT_LOGICAL, 0}};
  CHECK(divui_document_create(&config, &document));
  CHECK(divui_document_root(document, &root));
  CHECK(divui_element_create(document, DIVUI_ELEMENT_INPUT, &input));
  CHECK(divui_element_create(document, DIVUI_ELEMENT_INPUT, &box));
  CHECK(divui_element_set_attribute(box, text("type"), text("checkbox")));
  CHECK(divui_element_append_child(root, input));
  CHECK(divui_element_append_child(root, box));
  const DivUiEventType types[] = {DIVUI_EVENT_BEFORE_INPUT, DIVUI_EVENT_INPUT, DIVUI_EVENT_CHANGE};
  DivUiListener* listeners[12];
  CallbackData data[12];
  size_t count = 0;
  for (size_t node = 0; node < 2; ++node) {
    for (size_t type = 0; type < 3; ++type) {
      for (int modifier = 1; modifier >= 0; --modifier) {
        data[count].target = node ? "box" : "text";
        data[count].modifier = modifier;
        CHECK(divui_element_add_event_listener(node ? box : input, types[type], 0, callback,
                                               &data[count], &listeners[count]));
        ++count;
      }
    }
  }
  printf("{\"rows\":[");
  CHECK(divui_element_focus(input));
  CHECK(divui_document_dispatch_text_input_v1(document, text("A")));
  CHECK(divui_document_dispatch_text_input_v1(document, text("blocked")));
  DivUiEvent click;
  memset(&click, 0, sizeof(click));
  click.struct_size = sizeof(click);
  click.abi_version = DIVUI_ABI_VERSION;
  click.event_type = DIVUI_EVENT_CLICK;
  CHECK(divui_document_dispatch_event(document, box, &click));
  uint8_t value[16];
  size_t length = 0;
  CHECK(divui_element_copy_control_value(input, value, sizeof(value), &length));
  uint32_t flags = 0;
  CHECK(divui_element_get_control_flags(box, &flags));
  printf("],\"value\":\"%.*s\",\"checked\":%s}\n", (int)length, value,
         flags & DIVUI_CONTROL_CHECKED ? "true" : "false");
  for (size_t i = 0; i < count; ++i)
    CHECK(divui_listener_destroy(listeners[i]));
  CHECK(divui_element_destroy(box));
  CHECK(divui_element_destroy(input));
  CHECK(divui_element_destroy(root));
  CHECK(divui_document_destroy(document));
  return 0;
}
