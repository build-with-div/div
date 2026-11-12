/* Native C/C++ consumer of retained scroll-button ownership and interaction.
 * Keep all guards active in packaged release builds. */
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stddef.h>
#include <string.h>

#include "divui.h"

#ifdef __cplusplus
static_assert(sizeof(DivUiScrollButtonStateV1) == 56, "scroll-button ABI size");
static_assert(alignof(DivUiScrollButtonStateV1) == 8, "scroll-button ABI alignment");
static_assert(offsetof(DivUiScrollButtonStateV1, current_x) == 24, "scroll-button ABI offset");
#else
_Static_assert(sizeof(DivUiScrollButtonStateV1) == 56, "scroll-button ABI size");
_Static_assert(_Alignof(DivUiScrollButtonStateV1) == 8, "scroll-button ABI alignment");
_Static_assert(offsetof(DivUiScrollButtonStateV1, current_x) == 24, "scroll-button ABI offset");
#endif

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

static DivUiScrollButtonStateV1 snapshot(DivUiElement* button) {
  DivUiScrollButtonStateV1 value;
  memset(&value, 0, sizeof(value));
  value.struct_size = sizeof(value);
  value.abi_version = DIVUI_ABI_VERSION;
  assert(divui_scroll_button_get_state_v1(button, &value) == DIVUI_OK);
  return value;
}

static void same_node(DivUiElement* first, DivUiElement* second) {
  uint32_t same = 0;
  assert(divui_element_is_same_node_v1(first, second, &same) == DIVUI_OK);
  assert(same == 1);
}

typedef struct Callback {
  DivUiElement* origin;
  DivUiElement* button;
  unsigned clicks;
  unsigned changes;
  unsigned key_down;
  unsigned key_up;
  int cancel;
  int cancel_key;
} Callback;

static void clicked(DivUiEvent* event, void* data) {
  Callback* callback = (Callback*)data;
  same_node(event->target, callback->origin);
  same_node(event->current_target, callback->origin);
  if (event->event_type == DIVUI_EVENT_CLICK) {
    ++callback->clicks;
    /* The callback can read the same Engine after all dispatch borrows end. */
    DivUiScrollButtonStateV1 value = snapshot(callback->button);
    assert(value.current_y >= 0);
    if (callback->cancel)
      event->flags |= DIVUI_EVENT_DEFAULT_PREVENTED;
  } else if (event->event_type == DIVUI_EVENT_KEY_DOWN) {
    ++callback->key_down;
    /* The same retained Engine is readable before the key's default action. */
    DivUiScrollButtonStateV1 value = snapshot(callback->button);
    assert(value.current_y >= 0);
    if (callback->cancel_key)
      event->flags |= DIVUI_EVENT_DEFAULT_PREVENTED;
  } else if (event->event_type == DIVUI_EVENT_KEY_UP) {
    ++callback->key_up;
  } else {
    assert(event->event_type == DIVUI_EVENT_INPUT || event->event_type == DIVUI_EVENT_CHANGE);
    ++callback->changes;
  }
}

static void key(DivUiDocument* document, uint32_t kind, uint32_t code, const char* name) {
  DivUiEvent event;
  memset(&event, 0, sizeof(event));
  event.struct_size = sizeof(event);
  event.abi_version = DIVUI_ABI_VERSION;
  event.event_type = kind;
  event.key_code = code;
  event.text = text(name);
  assert(divui_document_dispatch_key_input_v1(document, &event, text("")) == DIVUI_OK);
}

static void run(double scale) {
  DivUiDocumentConfig config = {sizeof(config),
                                DIVUI_ABI_VERSION,
                                {320.0, 240.0, (uint32_t)(320.0 * scale), (uint32_t)(240.0 * scale),
                                 scale, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  DivUiElement* root = NULL;
  DivUiElement* origin = NULL;
  DivUiElement* content = NULL;
  DivUiElement* button = NULL;
  assert(divui_document_create(&config, &document) == DIVUI_OK);
  assert(divui_document_root(document, &root) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &origin) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &content) == DIVUI_OK);
  assert(divui_element_append_child(root, origin) == DIVUI_OK);
  assert(divui_element_append_child(origin, content) == DIVUI_OK);
  property(origin, DIVUI_STYLE_PROPERTY_WIDTH, "80px");
  property(origin, DIVUI_STYLE_PROPERTY_HEIGHT, "80px");
  property(origin, DIVUI_STYLE_PROPERTY_OVERFLOW, "hidden");
  property(origin, DIVUI_STYLE_PROPERTY_SCROLLBAR_WIDTH, "none");
  property(content, DIVUI_STYLE_PROPERTY_WIDTH, "80px");
  property(content, DIVUI_STYLE_PROPERTY_HEIGHT, "500px");
  assert(divui_element_create_scroll_button_v1(origin, DIVUI_SCROLL_BUTTON_DOWN, &button) ==
         DIVUI_OK);
  assert(divui_element_set_native_appearance_v1(button, 0) == DIVUI_OK);
  assert(divui_element_set_text(button, text("next")) == DIVUI_OK);
  property(button, DIVUI_STYLE_PROPERTY_WIDTH, "24px");
  property(button, DIVUI_STYLE_PROPERTY_HEIGHT, "24px");
  assert(divui_element_set_attribute(button, text("aria-label"), text("next page")) == DIVUI_OK);
  DivUiElement* alias = NULL;
  assert(divui_element_get_scroll_button_v1(origin, DIVUI_SCROLL_BUTTON_DOWN, &alias) == DIVUI_OK);
  assert(alias != NULL && alias != button);
  same_node(button, alias);
  /* Aliases own only handles; the Engine owns the retained control. */
  assert(divui_element_destroy(button) == DIVUI_OK);
  button = alias;
  DivUiElement* related = NULL;
  assert(divui_scroll_button_get_origin_v1(button, &related) == DIVUI_OK);
  same_node(origin, related);
  assert(divui_element_destroy(related) == DIVUI_OK);
  DivUiScrollButtonStateV1 initial = snapshot(button);
  assert(initial.enabled == 1 && initial.has_target == 1);
  assert(initial.current_y == 0 && initial.target_y == 70);
  assert(initial.direction == DIVUI_SCROLL_BUTTON_DOWN);
  size_t reason_length = 1;
  assert(divui_scroll_button_copy_unavailable_reason_v1(button, NULL, 0, &reason_length) ==
         DIVUI_OK);
  assert(reason_length == 0);
  reason_length = 37;
  assert(divui_scroll_button_copy_unavailable_reason_v1(button, NULL, 1, &reason_length) ==
         DIVUI_ERROR_INVALID_ARGUMENT);
  assert(reason_length == 37);
  related = (DivUiElement*)(uintptr_t)1;
  assert(divui_scroll_button_get_snap_target_v1(button, 1, DIVUI_SCROLL_BUTTON_PREVIEW_SNAP,
                                                &related) == DIVUI_OK);
  assert(related == NULL);
  related = (DivUiElement*)(uintptr_t)1;
  assert(divui_scroll_button_get_snap_target_v1(button, 2, DIVUI_SCROLL_BUTTON_CURRENT_SNAP,
                                                &related) == DIVUI_ERROR_INVALID_ARGUMENT);
  assert(related == (DivUiElement*)(uintptr_t)1);

  Callback callback = {origin, button, 0, 0, 0, 0, 1, 0};
  DivUiListener* listeners[5] = {NULL, NULL, NULL, NULL, NULL};
  const DivUiEventType kinds[5] = {DIVUI_EVENT_CLICK, DIVUI_EVENT_INPUT, DIVUI_EVENT_CHANGE,
                                   DIVUI_EVENT_KEY_DOWN, DIVUI_EVENT_KEY_UP};
  for (unsigned i = 0; i < 5; ++i)
    assert(divui_element_add_event_listener(origin, kinds[i], 0, clicked, &callback,
                                            &listeners[i]) == DIVUI_OK);
  assert(divui_scroll_button_activate_v1(button) == DIVUI_OK);
  assert(snapshot(button).current_y == 0);
  assert(callback.clicks == 1 && callback.changes == 0);
  assert(callback.key_down == 0 && callback.key_up == 0);
  callback.cancel = 0;
  assert(divui_scroll_button_activate_v1(button) == DIVUI_OK);
  assert(snapshot(button).current_y == 70);
  assert(callback.clicks == 2 && callback.changes == 0);
  DivUiElement* focused = NULL;
  assert(divui_document_focused_element_v1(document, &focused) == DIVUI_OK);
  assert(focused != NULL);
  same_node(origin, focused);
  assert(divui_element_destroy(focused) == DIVUI_OK);
  /* Pinned Chromium scroll-button defaults activate on keydown directly;
   * they do not synthesize a click or arm ordinary Button's Space-keyup path. */
  key(document, DIVUI_EVENT_KEY_DOWN, 32, " ");
  assert(snapshot(button).current_y == 140);
  assert(callback.clicks == 2 && callback.changes == 0);
  assert(callback.key_down == 1 && callback.key_up == 0);
  key(document, DIVUI_EVENT_KEY_UP, 32, " ");
  assert(snapshot(button).current_y == 140);
  assert(callback.clicks == 2 && callback.changes == 0);
  assert(callback.key_down == 1 && callback.key_up == 1);
  key(document, DIVUI_EVENT_KEY_DOWN, 13, "Enter");
  assert(snapshot(button).current_y == 210);
  assert(callback.clicks == 2 && callback.changes == 0);
  assert(callback.key_down == 2 && callback.key_up == 1);
  key(document, DIVUI_EVENT_KEY_UP, 13, "Enter");
  assert(snapshot(button).current_y == 210);
  assert(callback.clicks == 2 && callback.changes == 0);
  assert(callback.key_down == 2 && callback.key_up == 2);
  callback.cancel_key = 1;
  key(document, DIVUI_EVENT_KEY_DOWN, 32, " ");
  assert(snapshot(button).current_y == 210);
  assert(callback.clicks == 2 && callback.changes == 0);
  assert(callback.key_down == 3 && callback.key_up == 2);
  key(document, DIVUI_EVENT_KEY_UP, 32, " ");
  assert(snapshot(button).current_y == 210);
  assert(callback.clicks == 2 && callback.changes == 0);
  assert(callback.key_down == 3 && callback.key_up == 3);
  callback.cancel_key = 0;
  assert(divui_element_set_attribute(button, text("disabled"), text("")) == DIVUI_OK);
  DivUiScrollButtonStateV1 disabled = snapshot(button);
  assert(disabled.enabled == 0 && disabled.availability == DIVUI_SCROLL_BUTTON_AUTHORED_DISABLED);
  assert(divui_scroll_button_activate_v1(button) == DIVUI_OK);
  assert(snapshot(button).current_y == 210);
  assert(callback.clicks == 2 && callback.changes == 0);

  for (unsigned i = 0; i < 5; ++i)
    assert(divui_listener_destroy(listeners[i]) == DIVUI_OK);
  assert(divui_element_remove(origin) == DIVUI_OK);
  DivUiScrollButtonStateV1 stale = initial;
  assert(divui_scroll_button_get_state_v1(button, &stale) == DIVUI_ERROR_STALE_HANDLE);
  assert(memcmp(&stale, &initial, sizeof(stale)) == 0);
  assert(divui_element_destroy(button) == DIVUI_OK);
  assert(divui_element_destroy(content) == DIVUI_OK);
  assert(divui_element_destroy(origin) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(document) == DIVUI_OK);
  assert(initial.current_y == 0 && disabled.current_y == 210);
}

static DivUiElement* snap_area(DivUiDocument* document, DivUiElement* content, const char* x) {
  DivUiElement* area = NULL;
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &area) == DIVUI_OK);
  property(area, DIVUI_STYLE_PROPERTY_WIDTH, "20px");
  property(area, DIVUI_STYLE_PROPERTY_HEIGHT, "20px");
  property(area, DIVUI_STYLE_PROPERTY_POSITION, "absolute");
  property(area, DIVUI_STYLE_PROPERTY_LEFT, x);
  property(area, DIVUI_STYLE_PROPERTY_TOP, "0px");
  /* One property declaration carries the block and inline alignments. */
  property(area, DIVUI_STYLE_PROPERTY_SCROLL_SNAP_ALIGN, "end start");
  assert(divui_element_append_child(content, area) == DIVUI_OK);
  return area;
}

static void snap_target(DivUiElement* button, uint32_t phase, DivUiElement* expected) {
  DivUiElement* alias = NULL;
  assert(divui_scroll_button_get_snap_target_v1(button, 0, phase, &alias) == DIVUI_OK);
  assert(alias != NULL);
  same_node(alias, expected);
  assert(divui_element_destroy(alias) == DIVUI_OK);
}

static void run_snap(double scale) {
  DivUiDocumentConfig config = {sizeof(config),
                                DIVUI_ABI_VERSION,
                                {320.0, 240.0, (uint32_t)(320.0 * scale), (uint32_t)(240.0 * scale),
                                 scale, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  DivUiElement* root = NULL;
  DivUiElement* origin = NULL;
  DivUiElement* content = NULL;
  DivUiElement* button = NULL;
  assert(divui_document_create(&config, &document) == DIVUI_OK);
  assert(divui_document_root(document, &root) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &origin) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &content) == DIVUI_OK);
  assert(divui_element_append_child(root, origin) == DIVUI_OK);
  assert(divui_element_append_child(origin, content) == DIVUI_OK);
  property(origin, DIVUI_STYLE_PROPERTY_WIDTH, "100px");
  property(origin, DIVUI_STYLE_PROPERTY_HEIGHT, "80px");
  property(origin, DIVUI_STYLE_PROPERTY_OVERFLOW, "scroll");
  property(origin, DIVUI_STYLE_PROPERTY_SCROLLBAR_WIDTH, "none");
  property(origin, DIVUI_STYLE_PROPERTY_SCROLL_SNAP_AXIS, "x");
  property(content, DIVUI_STYLE_PROPERTY_WIDTH, "400px");
  property(content, DIVUI_STYLE_PROPERTY_HEIGHT, "300px");
  property(content, DIVUI_STYLE_PROPERTY_POSITION, "relative");
  DivUiElement* first = snap_area(document, content, "40px");
  DivUiElement* second = snap_area(document, content, "200px");
  DivUiElement* preferred = snap_area(document, content, "200px");
  assert(divui_element_create_scroll_button_v1(origin, DIVUI_SCROLL_BUTTON_RIGHT, &button) ==
         DIVUI_OK);
  assert(divui_element_set_native_appearance_v1(button, 0) == DIVUI_OK);
  property(button, DIVUI_STYLE_PROPERTY_WIDTH, "24px");
  property(button, DIVUI_STYLE_PROPERTY_HEIGHT, "24px");
  assert(snapshot(button).current_x == 40);
  snap_target(button, DIVUI_SCROLL_BUTTON_CURRENT_SNAP, first);

  assert(divui_element_set_scroll_snap_target_v1(origin, preferred) == DIVUI_OK);
  snap_target(button, DIVUI_SCROLL_BUTTON_PREVIEW_SNAP, preferred);
  DivUiDocument* foreign_document = NULL;
  DivUiElement* foreign = NULL;
  assert(divui_document_create(&config, &foreign_document) == DIVUI_OK);
  assert(divui_element_create(foreign_document, DIVUI_ELEMENT_DIV, &foreign) == DIVUI_OK);
  assert(divui_element_set_scroll_snap_target_v1(origin, foreign) == DIVUI_ERROR_WRONG_DOCUMENT);
  snap_target(button, DIVUI_SCROLL_BUTTON_PREVIEW_SNAP, preferred);
  assert(divui_element_destroy(foreign) == DIVUI_OK);
  assert(divui_document_destroy(foreign_document) == DIVUI_OK);

  assert(divui_scroll_button_activate_v1(button) == DIVUI_OK);
  assert(snapshot(button).current_x == 200);
  snap_target(button, DIVUI_SCROLL_BUTTON_CURRENT_SNAP, preferred);
  property(preferred, DIVUI_STYLE_PROPERTY_LEFT, "250px");
  assert(snapshot(button).current_x == 250);
  snap_target(button, DIVUI_SCROLL_BUTTON_CURRENT_SNAP, preferred);
  property(preferred, DIVUI_STYLE_PROPERTY_DISPLAY, "none");
  assert(snapshot(button).current_x == 200);
  snap_target(button, DIVUI_SCROLL_BUTTON_CURRENT_SNAP, second);
  assert(divui_element_set_scroll_snap_target_v1(origin, NULL) == DIVUI_OK);
  uint8_t changed = 37;
  assert(divui_element_settle_scroll_snap_v1(origin, &changed) == DIVUI_OK);
  assert(changed == 0 && snapshot(button).current_x == 200);
  assert(divui_element_settle_scroll_snap_v1(origin, NULL) == DIVUI_ERROR_INVALID_ARGUMENT);
  assert(divui_element_remove(origin) == DIVUI_OK);
  changed = 37;
  assert(divui_element_settle_scroll_snap_v1(origin, &changed) == DIVUI_ERROR_STALE_HANDLE);
  assert(changed == 37);

  assert(divui_element_destroy(preferred) == DIVUI_OK);
  assert(divui_element_destroy(second) == DIVUI_OK);
  assert(divui_element_destroy(first) == DIVUI_OK);
  assert(divui_element_destroy(button) == DIVUI_OK);
  assert(divui_element_destroy(content) == DIVUI_OK);
  assert(divui_element_destroy(origin) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(document) == DIVUI_OK);
}

int main(void) {
  const double scales[] = {1, 1.25, 1.5, 2, 3};
  for (unsigned i = 0; i < sizeof(scales) / sizeof(scales[0]); ++i) {
    run(scales[i]);
    run_snap(scales[i]);
  }
  return 0;
}
