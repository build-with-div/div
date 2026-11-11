/* Checked active-state query, callback mutation and handle lifetime guards. */
#include "divui.h"
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "portable_thread.h"
#define OK(call) assert((call) == DIVUI_OK)
static DivUiUtf8 text(const char* s) {
  DivUiUtf8 result = {(const uint8_t*)s, strlen(s)};
  return result;
}
static void reject(DivUiElement* node, DivUiStatus status) {
  uint32_t value = 71;
  assert(divui_element_is_active_v1(node, &value) == status);
  assert(value == 71);
}
static void* foreign_thread(void* data) {
  reject((DivUiElement*)data, DIVUI_ERROR_WRONG_THREAD);
  return NULL;
}
typedef struct State {
  DivUiElement* node;
  unsigned calls;
} State;
static void callback(DivUiEvent* event, void* data) {
  State* s = (State*)data;
  assert(event->event_type == DIVUI_EVENT_POINTER_DOWN);
  uint32_t active = 0;
  OK(divui_element_is_active_v1(s->node, &active));
  assert(active == 1);
  OK(divui_element_set_attribute(s->node, text("data-callback"), text("mutated")));
  OK(divui_element_is_active_v1(s->node, &active));
  assert(active == 1);
  ++s->calls;
}
static void property(DivUiElement* node, DivUiStyleProperty property_, const char* literal) {
  DivUiStyleValue value;
  OK(divui_style_value_parse(property_, text(literal), &value));
  OK(divui_element_set_property(node, property_, &value));
  if (value.tag == DIVUI_STYLE_VALUE_COMPOUND)
    OK(divui_style_compound_destroy((DivUiStyleCompound*)value.data.compound));
}
int main(void) {
  DivUiDocumentConfig config = {
      sizeof(config), DIVUI_ABI_VERSION, {100, 100, 100, 100, 1, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  DivUiElement *root = NULL, *button = NULL;
  OK(divui_document_create(&config, &document));
  OK(divui_document_root(document, &root));
  OK(divui_element_create(document, DIVUI_ELEMENT_BUTTON, &button));
  property(button, DIVUI_STYLE_PROPERTY_DISPLAY, "block");
  property(button, DIVUI_STYLE_PROPERTY_WIDTH, "40px");
  property(button, DIVUI_STYLE_PROPERTY_HEIGHT, "30px");
  OK(divui_element_append_child(root, button));
  OK(divui_document_update(document));
  uint32_t active = 99;
  OK(divui_element_is_active_v1(button, &active));
  assert(!active);
  State state = {button, 0};
  DivUiListener* listener = NULL;
  OK(divui_element_add_event_listener(button, DIVUI_EVENT_POINTER_DOWN, 0, callback, &state,
                                      &listener));
  DivUiEvent event;
  memset(&event, 0, sizeof(event));
  event.struct_size = sizeof(event);
  event.abi_version = DIVUI_ABI_VERSION;
  event.event_type = DIVUI_EVENT_POINTER_DOWN;
  event.x = 10;
  event.y = 10;
  event.pointer_id = 7;
  OK(divui_document_dispatch_pointer_event(document, &event));
  assert(state.calls == 1);
  OK(divui_element_is_active_v1(button, &active));
  assert(active == 1);
  event.event_type = DIVUI_EVENT_POINTER_UP;
  event.flags = 0;
  OK(divui_document_dispatch_pointer_event(document, &event));
  OK(divui_element_is_active_v1(button, &active));
  assert(!active);
  OK(divui_listener_destroy(listener));
  reject(NULL, DIVUI_ERROR_INVALID_ARGUMENT);
  reject((DivUiElement*)(uintptr_t)SIZE_MAX, DIVUI_ERROR_INVALID_HANDLE);
  reject((DivUiElement*)document, DIVUI_ERROR_INVALID_HANDLE);
  assert(divui_element_is_active_v1(button, NULL) == DIVUI_ERROR_INVALID_ARGUMENT);
  pthread_t thread;
  assert(!pthread_create(&thread, NULL, foreign_thread, button));
  assert(!pthread_join(thread, NULL));
  OK(divui_element_remove(button));
  reject(button, DIVUI_ERROR_STALE_HANDLE);
  OK(divui_element_destroy(button));
  reject(button, DIVUI_ERROR_INVALID_HANDLE);
  OK(divui_document_destroy(document));
  reject(root, DIVUI_ERROR_INVALID_HANDLE);
  puts("native active-state callback and ownership guards passed");
  return 0;
}
