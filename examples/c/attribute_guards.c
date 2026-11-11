/* C/C++ native attribute lookup, owned data and handle lifetime guards. */
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
static DivUiUtf8 text(const char* value) {
  DivUiUtf8 result = {(const uint8_t*)value, strlen(value)};
  return result;
}
static void equal(DivUiBuffer* value, const char* expected) {
  assert(value != NULL);
  assert(divui_buffer_length(value) == strlen(expected));
  if (*expected)
    assert(memcmp(divui_buffer_data(value), expected, strlen(expected)) == 0);
}
static void rejected(DivUiElement* element, DivUiUtf8 name, DivUiStatus expected) {
  DivUiBuffer* output = (DivUiBuffer*)(uintptr_t)1;
  assert(divui_element_get_attribute_v1(element, name, &output) == expected);
  assert(output == (DivUiBuffer*)(uintptr_t)1);
}
static void* wrong_thread(void* element) {
  rejected((DivUiElement*)element, text("data-text"), DIVUI_ERROR_WRONG_THREAD);
  return NULL;
}
int main(void) {
  DivUiDocumentConfig config = {
      sizeof(config), DIVUI_ABI_VERSION, {320, 200, 320, 200, 1.0, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document;
  DivUiElement *root, *element;
  OK(divui_document_create(&config, &document));
  OK(divui_document_root(document, &root));
  OK(divui_element_create(document, DIVUI_ELEMENT_INPUT, &element));
  OK(divui_element_append_child(root, element));
  DivUiBuffer* value = (DivUiBuffer*)(uintptr_t)1;
  OK(divui_element_get_attribute_v1(element, text("absent"), &value));
  assert(value == NULL);
  OK(divui_element_get_attribute_v1(element, text(""), &value));
  assert(value == NULL);
  OK(divui_element_set_attribute(element, text("DiSaBlEd"), text("")));
  OK(divui_element_get_attribute_v1(element, text("DISABLED"), &value));
  equal(value, "");
  OK(divui_buffer_destroy(value));
  OK(divui_element_remove_attribute(element, text("disabled")));
  OK(divui_element_get_attribute_v1(element, text("disabled"), &value));
  assert(value == NULL);
  OK(divui_element_set_attribute(element, text("data-text"), text("界🙂é")));
  DivUiBuffer* saved = NULL;
  OK(divui_element_get_attribute_v1(element, text("DATA-TEXT"), &saved));
  equal(saved, "界🙂é");
  OK(divui_element_set_attribute(element, text("data-text"), text("changed")));
  OK(divui_element_get_attribute_v1(element, text("data-text"), &value));
  equal(value, "changed");
  equal(saved, "界🙂é");
  OK(divui_buffer_destroy(value));
  const uint8_t bad_byte = 0xff;
  DivUiUtf8 bad = {&bad_byte, 1};
  rejected(element, bad, DIVUI_ERROR_INVALID_ARGUMENT);
  bad.data = NULL;
  rejected(element, bad, DIVUI_ERROR_INVALID_ARGUMENT);
  rejected(NULL, text("data-text"), DIVUI_ERROR_INVALID_ARGUMENT);
  rejected((DivUiElement*)(uintptr_t)SIZE_MAX, text("data-text"), DIVUI_ERROR_INVALID_HANDLE);
  assert(divui_element_get_attribute_v1(element, text("data-text"), NULL) ==
         DIVUI_ERROR_INVALID_ARGUMENT);
  pthread_t thread;
  assert(pthread_create(&thread, NULL, wrong_thread, element) == 0);
  assert(pthread_join(thread, NULL) == 0);
  OK(divui_element_detach(element));
  OK(divui_element_get_attribute_v1(element, text("data-text"), &value));
  equal(value, "changed");
  OK(divui_buffer_destroy(value));
  OK(divui_element_remove(element));
  rejected(element, text("data-text"), DIVUI_ERROR_STALE_HANDLE);
  OK(divui_element_destroy(element));
  rejected(element, text("data-text"), DIVUI_ERROR_INVALID_HANDLE);
  OK(divui_element_destroy(root));
  OK(divui_document_destroy(document));
  equal(saved, "界🙂é");
  OK(divui_buffer_destroy(saved));
  assert(divui_buffer_destroy(saved) == DIVUI_ERROR_INVALID_HANDLE);
  puts(
      "native attribute presence/unicode/mutation/thread/stale/owned guards "
      "passed");
  return 0;
}
