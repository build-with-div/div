/* Native input metadata lifetime and ownership guards. */
#include "divui.h"
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>

#include "portable_thread.h"
#ifdef __cplusplus
static_assert(sizeof(DivUiInputEventInfoV1) == 40, "input info layout");
static_assert(offsetof(DivUiInputEventInfoV1, data) == 32, "input data offset");
#else
_Static_assert(sizeof(DivUiInputEventInfoV1) == 40, "input info layout");
_Static_assert(offsetof(DivUiInputEventInfoV1, data) == 32, "input data offset");
#endif
#define OK(call) assert((call) == DIVUI_OK)
static DivUiUtf8 text(const char* s) {
  DivUiUtf8 t = {(const uint8_t*)s, strlen(s)};
  return t;
}
static DivUiInputEventInfoV1 empty_info(void) {
  DivUiInputEventInfoV1 i;
  memset(&i, 0, sizeof(i));
  i.struct_size = sizeof(i);
  i.abi_version = DIVUI_ABI_VERSION;
  return i;
}
static void exact_data(DivUiInputEventInfoV1 i, const char* s) {
  assert(i.has_data == 1 && i.data != NULL);
  assert(divui_buffer_length(i.data) == strlen(s));
  assert(memcmp(divui_buffer_data(i.data), s, strlen(s)) == 0);
}
typedef struct Context {
  DivUiDocument *outer, *inner;
  const DivUiEvent *outer_event, *last_event;
  DivUiBuffer* saved;
  unsigned outer_rows, inner_rows, nesting;
} Context;
typedef struct ThreadCheck {
  const DivUiEvent* event;
  DivUiBuffer* buffer;
} ThreadCheck;
static void* other_thread(void* raw) {
  ThreadCheck* t = (ThreadCheck*)raw;
  DivUiInputEventInfoV1 i = empty_info(), original = i;
  assert(divui_event_input_info_v1(t->event, &i) == DIVUI_ERROR_INVALID_STATE);
  assert(memcmp(&i, &original, sizeof(i)) == 0);
  assert(divui_buffer_destroy(t->buffer) == DIVUI_ERROR_WRONG_THREAD);
  return NULL;
}
static void reject(DivUiEvent* event, DivUiInputEventInfoV1 i, DivUiStatus status) {
  DivUiInputEventInfoV1 original = i;
  assert(divui_event_input_info_v1(event, &i) == status);
  assert(memcmp(&i, &original, sizeof(i)) == 0);
}
static void inner_callback(DivUiEvent* event, void* raw) {
  Context* c = (Context*)raw;
  assert(c->nesting == 1);
  DivUiInputEventInfoV1 inner = empty_info(), outer = empty_info();
  OK(divui_event_input_info_v1(event, &inner));
  exact_data(inner, "B");
  OK(divui_event_input_info_v1(c->outer_event, &outer));
  exact_data(outer, "A");
  assert(inner.input_type == DIVUI_INPUT_INSERT_TEXT &&
         outer.input_type == DIVUI_INPUT_INSERT_TEXT);
  assert(inner.cancelable == (event->event_type == DIVUI_EVENT_BEFORE_INPUT));
  assert(outer.cancelable == 1);
  OK(divui_buffer_destroy(inner.data));
  OK(divui_buffer_destroy(outer.data));
  c->inner_rows++;
}
static void outer_callback(DivUiEvent* event, void* raw) {
  Context* c = (Context*)raw;
  DivUiInputEventInfoV1 i = empty_info();
  OK(divui_event_input_info_v1(event, &i));
  exact_data(i, "A");
  assert(i.input_type == DIVUI_INPUT_INSERT_TEXT && i.bubbles == 1 && i.is_composing == 0);
  assert(i.cancelable == (event->event_type == DIVUI_EVENT_BEFORE_INPUT));
  c->last_event = event;
  c->outer_rows++;
  if (event->event_type == DIVUI_EVENT_BEFORE_INPUT) {
    c->outer_event = event;
    c->nesting = 1;
    DivUiInputEventInfoV1 bad = empty_info();
    bad.struct_size = 4;
    reject(event, bad, DIVUI_ERROR_INVALID_ARGUMENT);
    bad = empty_info();
    bad.abi_version++;
    reject(event, bad, DIVUI_ERROR_ABI_MISMATCH);
    bad = empty_info();
    bad.data = i.data;
    reject(event, bad, DIVUI_ERROR_INVALID_ARGUMENT);
    assert(divui_event_input_info_v1(event, NULL) == DIVUI_ERROR_INVALID_ARGUMENT);
    reject(NULL, empty_info(), DIVUI_ERROR_INVALID_STATE);
    ThreadCheck t = {event, i.data};
    pthread_t thread;
    assert(pthread_create(&thread, NULL, other_thread, &t) == 0);
    assert(pthread_join(thread, NULL) == 0);
    exact_data(i, "A");
    OK(divui_document_dispatch_text_input_v1(c->inner, text("B")));
    c->nesting = 0;
    DivUiInputEventInfoV1 after = empty_info();
    OK(divui_event_input_info_v1(event, &after));
    exact_data(after, "A");
    OK(divui_buffer_destroy(after.data));
    c->saved = i.data;
  } else {
    OK(divui_buffer_destroy(i.data));
  }
}
static void setup(DivUiDocument** document, DivUiElement** root, DivUiElement** field) {
  DivUiDocumentConfig config = {
      sizeof(config), DIVUI_ABI_VERSION, {320, 200, 320, 200, 1.0, DIVUI_VIEWPORT_LOGICAL, 0}};
  OK(divui_document_create(&config, document));
  OK(divui_document_root(*document, root));
  OK(divui_element_create(*document, DIVUI_ELEMENT_INPUT, field));
  OK(divui_element_append_child(*root, *field));
  OK(divui_element_focus(*field));
}
int main(void) {
  Context c;
  memset(&c, 0, sizeof(c));
  DivUiElement *outer_root, *outer_field, *inner_root, *inner_field;
  DivUiListener* listeners[4];
  setup(&c.outer, &outer_root, &outer_field);
  setup(&c.inner, &inner_root, &inner_field);
  OK(divui_element_add_event_listener(outer_field, DIVUI_EVENT_BEFORE_INPUT, 0, outer_callback, &c,
                                      &listeners[0]));
  OK(divui_element_add_event_listener(outer_field, DIVUI_EVENT_INPUT, 0, outer_callback, &c,
                                      &listeners[1]));
  OK(divui_element_add_event_listener(inner_field, DIVUI_EVENT_BEFORE_INPUT, 0, inner_callback, &c,
                                      &listeners[2]));
  OK(divui_element_add_event_listener(inner_field, DIVUI_EVENT_INPUT, 0, inner_callback, &c,
                                      &listeners[3]));
  OK(divui_document_dispatch_text_input_v1(c.outer, text("A")));
  assert(c.outer_rows == 2 && c.inner_rows == 2 && c.nesting == 0 && c.saved != NULL);
  reject((DivUiEvent*)c.last_event, empty_info(), DIVUI_ERROR_INVALID_STATE);
  for (unsigned n = 0; n < 4; ++n)
    OK(divui_listener_destroy(listeners[n]));
  OK(divui_element_destroy(outer_field));
  OK(divui_element_destroy(outer_root));
  OK(divui_document_destroy(c.outer));
  OK(divui_element_destroy(inner_field));
  OK(divui_element_destroy(inner_root));
  OK(divui_document_destroy(c.inner));
  DivUiInputEventInfoV1 owned = empty_info();
  owned.has_data = 1;
  owned.data = c.saved;
  exact_data(owned, "A");
  OK(divui_buffer_destroy(c.saved));
  assert(divui_buffer_destroy(c.saved) == DIVUI_ERROR_INVALID_HANDLE);
  puts("native input metadata nested/thread/header/stale/owned guards passed");
  return 0;
}
