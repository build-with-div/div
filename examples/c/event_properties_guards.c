/* C/C++ consuming app: native callback properties and lifetime guards. */
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
#ifdef __cplusplus
static_assert(sizeof(DivUiEventPropertiesV1) == 16, "event properties layout");
#else
_Static_assert(sizeof(DivUiEventPropertiesV1) == 16, "event properties layout");
#endif

typedef struct Context {
  DivUiElement* a;
  DivUiElement* b;
  const DivUiEvent* outer;
  const DivUiEvent* last;
  unsigned rows;
  unsigned nested_rows;
  unsigned nesting;
} Context;

static DivUiEventPropertiesV1 properties(void) {
  DivUiEventPropertiesV1 result = {sizeof(result), DIVUI_ABI_VERSION, 99, 99};
  return result;
}

static void reject(const DivUiEvent* event, DivUiEventPropertiesV1 value, DivUiStatus expected) {
  DivUiEventPropertiesV1 original = value;
  assert(divui_event_properties_v1(event, &value) == expected);
  assert(memcmp(&value, &original, sizeof(value)) == 0);
}

static void observe(const DivUiEvent* event, uint32_t bubbles) {
  struct Extended {
    DivUiEventPropertiesV1 value;
    unsigned char trailing[16];
  } extended;
  extended.value = properties();
  memset(extended.trailing, 0x6d, sizeof(extended.trailing));
  extended.value.struct_size = sizeof(extended);
  OK(divui_event_properties_v1(event, &extended.value));
  assert(extended.value.struct_size == sizeof(extended.value));
  assert(extended.value.abi_version == DIVUI_ABI_VERSION);
  assert(extended.value.bubbles == bubbles && extended.value.cancelable == 0);
  for (size_t i = 0; i < sizeof(extended.trailing); ++i)
    assert(extended.trailing[i] == 0x6d);
}

static void* wrong_thread(void* address) {
  reject((const DivUiEvent*)address, properties(), DIVUI_ERROR_INVALID_STATE);
  return NULL;
}

static void callback(DivUiEvent* event, void* data) {
  Context* context = (Context*)data;
  uint32_t bubbles =
      event->event_type == DIVUI_EVENT_FOCUS_IN || event->event_type == DIVUI_EVENT_FOCUS_OUT;
  observe(event, bubbles);
  context->last = event;
  ++context->rows;
  if (context->nesting) {
    observe(context->outer, 0);
    ++context->nested_rows;
  }
  uint32_t target_a = 0;
  OK(divui_element_is_same_node_v1(event->target, context->a, &target_a));
  if (target_a && event->event_type == DIVUI_EVENT_FOCUS) {
    DivUiEventPropertiesV1 bad = properties();
    bad.struct_size = sizeof(uint32_t);
    reject(event, bad, DIVUI_ERROR_INVALID_ARGUMENT);
    bad = properties();
    bad.abi_version++;
    reject(event, bad, DIVUI_ERROR_ABI_MISMATCH);
    bad = properties();
    bad.struct_size = sizeof(bad) - 1;
    reject(event, bad, DIVUI_ERROR_INVALID_ARGUMENT);
    assert(divui_event_properties_v1(event, NULL) == DIVUI_ERROR_INVALID_ARGUMENT);
    reject(NULL, properties(), DIVUI_ERROR_INVALID_STATE);
    reject((const DivUiEvent*)(uintptr_t)1, properties(), DIVUI_ERROR_INVALID_STATE);
    uint32_t short_header = sizeof(uint32_t);
    assert(divui_event_properties_v1(event, (DivUiEventPropertiesV1*)&short_header) ==
           DIVUI_ERROR_INVALID_ARGUMENT);
    assert(short_header == sizeof(uint32_t));
    pthread_t thread;
    assert(pthread_create(&thread, NULL, wrong_thread, event) == 0);
    assert(pthread_join(thread, NULL) == 0);
    context->outer = event;
    context->nesting = 1;
    OK(divui_element_focus(context->b));
    context->nesting = 0;
    observe(event, 0);
  }
}

int main(void) {
  Context context;
  memset(&context, 0, sizeof(context));
  DivUiDocumentConfig config = {
      sizeof(config), DIVUI_ABI_VERSION, {320, 200, 320, 200, 1.0, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document;
  DivUiElement* root;
  DivUiListener* listeners[6];
  DivUiElement *alias_a, *alias_b;
  const uint8_t id_name[] = "id";
  const uint8_t id_a[] = "a", id_b[] = "b";
  DivUiUtf8 name = {id_name, 2}, a_id = {id_a, 1}, b_id = {id_b, 1};
  OK(divui_document_create(&config, &document));
  OK(divui_document_root(document, &root));
  OK(divui_element_create(document, DIVUI_ELEMENT_INPUT, &context.a));
  OK(divui_element_create(document, DIVUI_ELEMENT_INPUT, &context.b));
  OK(divui_element_append_child(root, context.a));
  OK(divui_element_append_child(root, context.b));
  unsigned count = 0;
  const DivUiEventType kinds[] = {DIVUI_EVENT_FOCUS, DIVUI_EVENT_FOCUS_IN, DIVUI_EVENT_BLUR,
                                  DIVUI_EVENT_FOCUS_OUT};
  for (unsigned i = 0; i < 4; ++i)
    OK(divui_element_add_event_listener(context.a, kinds[i], 0, callback, &context,
                                        &listeners[count++]));
  for (unsigned i = 0; i < 2; ++i)
    OK(divui_element_add_event_listener(context.b, kinds[i], 0, callback, &context,
                                        &listeners[count++]));
  /* Independent owned aliases become the callback target handles. Native node
   * identity must still resolve them to the listener's retained element. */
  OK(divui_element_set_attribute(context.a, name, a_id));
  OK(divui_element_set_attribute(context.b, name, b_id));
  OK(divui_document_element_by_id(document, a_id, &alias_a));
  OK(divui_document_element_by_id(document, b_id, &alias_b));
  assert(alias_a != context.a && alias_b != context.b);
  reject(NULL, properties(), DIVUI_ERROR_INVALID_STATE);
  OK(divui_element_focus(context.a));
  assert(context.rows == 5 && context.nested_rows == 4 && context.nesting == 0);
  reject(context.outer, properties(), DIVUI_ERROR_INVALID_STATE);
  reject(context.last, properties(), DIVUI_ERROR_INVALID_STATE);
  for (unsigned i = 0; i < count; ++i)
    OK(divui_listener_destroy(listeners[i]));
  OK(divui_element_destroy(alias_a));
  OK(divui_element_destroy(alias_b));
  OK(divui_element_destroy(context.a));
  OK(divui_element_destroy(context.b));
  OK(divui_element_destroy(root));
  OK(divui_document_destroy(document));
  puts(
      "native event properties nested/thread/header/stale/output guards "
      "passed");
  return 0;
}
