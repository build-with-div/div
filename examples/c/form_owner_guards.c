/* Independently owned C/C++ relation handles and invalid-input guards. */
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
typedef DivUiStatus (*Query)(DivUiElement*, DivUiElement**);
static const Query queries[] = {divui_element_associated_form_v1, divui_element_parent_v1};
static DivUiUtf8 text(const char* s) {
  DivUiUtf8 out = {(const uint8_t*)s, strlen(s)};
  return out;
}
static void reject(Query query, DivUiElement* element, DivUiStatus expected) {
  DivUiElement* output = (DivUiElement*)(uintptr_t)1;
  assert(query(element, &output) == expected);
  assert(output == (DivUiElement*)(uintptr_t)1);
}
static void* foreign_thread(void* node) {
  for (size_t i = 0; i < 2; ++i)
    reject(queries[i], (DivUiElement*)node, DIVUI_ERROR_WRONG_THREAD);
  return NULL;
}
static void callback(DivUiEvent* event, void* data) {
  DivUiElement *input = (DivUiElement*)data, *form = NULL, *parent = NULL;
  (void)event;
  OK(divui_element_associated_form_v1(input, &form));
  OK(divui_element_parent_v1(input, &parent));
  assert(form && parent);
  uint32_t same = 0;
  OK(divui_element_is_same_node_v1(form, parent, &same));
  assert(same);
  OK(divui_element_set_attribute(form, text("id"), text("callback")));
  OK(divui_element_destroy(form));
  OK(divui_element_destroy(parent));
}
int main(void) {
  DivUiDocumentConfig config = {
      sizeof(config), DIVUI_ABI_VERSION, {320, 200, 320, 200, 1, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  DivUiElement *body = NULL, *form = NULL, *input = NULL;
  OK(divui_document_create(&config, &document));
  OK(divui_document_root(document, &body));
  OK(divui_element_create(document, DIVUI_ELEMENT_FORM, &form));
  OK(divui_element_create(document, DIVUI_ELEMENT_INPUT, &input));
  OK(divui_element_append_child(body, form));
  OK(divui_element_append_child(form, input));
  DivUiElement *owner = NULL, *parent = NULL;
  OK(divui_element_associated_form_v1(input, &owner));
  OK(divui_element_parent_v1(input, &parent));
  assert(owner && parent && owner != form && parent != form && owner != parent);
  uint32_t same = 0;
  OK(divui_element_is_same_node_v1(owner, form, &same));
  assert(same);
  OK(divui_element_is_same_node_v1(parent, form, &same));
  assert(same);
  OK(divui_element_destroy(owner));
  OK(divui_element_set_attribute(parent, text("id"), text("retained")));
  DivUiListener* listener = NULL;
  OK(divui_element_add_event_listener(input, DIVUI_EVENT_CLICK, 0, callback, input, &listener));
  OK(divui_element_perform_accessibility_action(input, DIVUI_ACCESSIBILITY_CLICK, text(""), 0, 0));
  OK(divui_listener_destroy(listener));
  for (size_t i = 0; i < 2; ++i) {
    reject(queries[i], NULL, DIVUI_ERROR_INVALID_ARGUMENT);
    reject(queries[i], (DivUiElement*)(uintptr_t)SIZE_MAX, DIVUI_ERROR_INVALID_HANDLE);
    assert(queries[i](input, NULL) == DIVUI_ERROR_INVALID_ARGUMENT);
    reject(queries[i], (DivUiElement*)document, DIVUI_ERROR_INVALID_HANDLE);
  }
  pthread_t thread;
  assert(pthread_create(&thread, NULL, foreign_thread, input) == 0);
  assert(pthread_join(thread, NULL) == 0);
  DivUiElement* absent = (DivUiElement*)(uintptr_t)1;
  OK(divui_element_associated_form_v1(body, &absent));
  assert(!absent);
  OK(divui_element_detach(form));
  OK(divui_element_associated_form_v1(input, &owner));
  OK(divui_element_destroy(owner));
  OK(divui_element_destroy(form)); /* Other aliases still refer to the live form. */
  OK(divui_element_set_attribute(parent, text("id"), text("alias")));
  OK(divui_element_remove(input));
  for (size_t i = 0; i < 2; ++i)
    reject(queries[i], input, DIVUI_ERROR_STALE_HANDLE);
  OK(divui_element_destroy(input));
  for (size_t i = 0; i < 2; ++i)
    reject(queries[i], input, DIVUI_ERROR_INVALID_HANDLE);
  OK(divui_document_destroy(document));
  for (size_t i = 0; i < 2; ++i)
    reject(queries[i], parent, DIVUI_ERROR_INVALID_HANDLE);
  puts("native form/parent owned relation guards passed");
  return 0;
}
