/* Keep consuming-app checks active in packaged release builds. */
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "divui.h"
#include "portable_thread.h"

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

typedef struct ScrollObserver {
  DivUiElement* element;
  unsigned calls;
} ScrollObserver;

static void scrolled(DivUiEvent* event, void* data) {
  ScrollObserver* observer = (ScrollObserver*)data;
  assert(event->event_type == DIVUI_EVENT_SCROLL);
  double x = -1.0, y = -1.0;
  /* The same document can be borrowed again during the application callback. */
  assert(divui_element_get_scroll_offset(observer->element, &x, &y) == DIVUI_OK);
  assert(x >= 0.0 && y >= 0.0);
  ++observer->calls;
}

static void drain(DivUiDocument* document) {
  uint32_t pending = 1;
  for (unsigned attempt = 0; pending && attempt < 16; ++attempt)
    assert(divui_document_dispatch_pending_events_v1(document, &pending) == DIVUI_OK);
  assert(pending == 0);
}

static void offset(DivUiElement* element, double expected_x, double expected_y) {
  double x = -1.0, y = -1.0;
  assert(divui_element_get_scroll_offset(element, &x, &y) == DIVUI_OK);
  assert(x == expected_x && y == expected_y);
}

static void* foreign_action(void* pointer) {
  DivUiElement* element = (DivUiElement*)pointer;
  assert(divui_element_perform_accessibility_action(element, DIVUI_ACCESSIBILITY_SCROLL_DOWN,
                                                    text(""), 0, 0) == DIVUI_ERROR_WRONG_THREAD);
  return NULL;
}

static void exercise(double scale) {
  DivUiDocumentConfig config = {sizeof(config),
                                DIVUI_ABI_VERSION,
                                {320.0, 240.0, (uint32_t)(320.0 * scale), (uint32_t)(240.0 * scale),
                                 scale, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  DivUiElement* root = NULL;
  DivUiElement* viewport = NULL;
  DivUiElement* content = NULL;
  assert(divui_document_create(&config, &document) == DIVUI_OK);
  assert(divui_document_root(document, &root) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &viewport) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &content) == DIVUI_OK);
  assert(divui_element_append_child(root, viewport) == DIVUI_OK);
  assert(divui_element_append_child(viewport, content) == DIVUI_OK);
  property(viewport, DIVUI_STYLE_PROPERTY_WIDTH, "100px");
  property(viewport, DIVUI_STYLE_PROPERTY_HEIGHT, "100px");
  property(viewport, DIVUI_STYLE_PROPERTY_OVERFLOW, "auto");
  property(viewport, DIVUI_STYLE_PROPERTY_SCROLLBAR_WIDTH, "none");
  property(content, DIVUI_STYLE_PROPERTY_WIDTH, "300px");
  property(content, DIVUI_STYLE_PROPERTY_HEIGHT, "300px");
  offset(viewport, 0.0, 0.0);

  uint64_t id = 0;
  assert(divui_element_get_accessibility_id(viewport, &id) == DIVUI_OK);
  DivUiAccessibilitySnapshot* first = NULL;
  assert(divui_document_accessibility_snapshot(document, NULL, &first) == DIVUI_OK);
  DivUiAccessibilitySnapshotInfo info;
  memset(&info, 0, sizeof(info));
  info.struct_size = sizeof(info);
  info.abi_version = DIVUI_ABI_VERSION;
  assert(divui_accessibility_snapshot_get_info(first, &info) == DIVUI_OK);
  unsigned found = 0;
  for (size_t index = 0; index < info.node_count; ++index) {
    DivUiAccessibilityNodeInfo node;
    memset(&node, 0, sizeof(node));
    node.struct_size = sizeof(node);
    node.abi_version = DIVUI_ABI_VERSION;
    assert(divui_accessibility_snapshot_get_node(first, index, &node) == DIVUI_OK);
    if (node.id == id) {
      const uint32_t directions =
          DIVUI_ACCESSIBILITY_ACTION_SCROLL_DOWN_BIT | DIVUI_ACCESSIBILITY_ACTION_SCROLL_UP_BIT |
          DIVUI_ACCESSIBILITY_ACTION_SCROLL_LEFT_BIT | DIVUI_ACCESSIBILITY_ACTION_SCROLL_RIGHT_BIT;
      assert((node.actions & directions) == directions);
      ++found;
    }
  }
  assert(found == 1);

  ScrollObserver observer = {viewport, 0};
  DivUiListener* listener = NULL;
  assert(divui_element_add_event_listener(viewport, DIVUI_EVENT_SCROLL, 0, scrolled, &observer,
                                          &listener) == DIVUI_OK);
  pthread_t foreign;
  assert(pthread_create(&foreign, NULL, foreign_action, viewport) == 0);
  assert(pthread_join(foreign, NULL) == 0);
  assert(divui_element_perform_accessibility_action(viewport, (DivUiAccessibilityAction)99,
                                                    text(""), 0,
                                                    0) == DIVUI_ERROR_INVALID_ARGUMENT);
  offset(viewport, 0.0, 0.0);
  drain(document);
  assert(observer.calls == 0);

  const DivUiAccessibilityAction actions[] = {
      DIVUI_ACCESSIBILITY_SCROLL_DOWN, DIVUI_ACCESSIBILITY_SCROLL_RIGHT,
      DIVUI_ACCESSIBILITY_SCROLL_UP, DIVUI_ACCESSIBILITY_SCROLL_LEFT};
  const double positions[][2] = {{0.0, 80.0}, {80.0, 80.0}, {80.0, 0.0}, {0.0, 0.0}};
  for (unsigned index = 0; index < 4; ++index) {
    assert(divui_element_perform_accessibility_action(viewport, actions[index], text(""), 0, 0) ==
           DIVUI_OK);
    offset(viewport, positions[index][0], positions[index][1]);
    assert(observer.calls == index);
    drain(document);
    assert(observer.calls == index + 1);
  }
  assert(divui_element_perform_accessibility_action(viewport, DIVUI_ACCESSIBILITY_SCROLL_UP,
                                                    text(""), 0, 0) == DIVUI_OK);
  assert(divui_element_perform_accessibility_action(viewport, DIVUI_ACCESSIBILITY_SCROLL_LEFT,
                                                    text(""), 0, 0) == DIVUI_OK);
  offset(viewport, 0.0, 0.0);
  drain(document);
  assert(observer.calls == 4);

  assert(divui_element_scroll_to(viewport, 190.0, 190.0) == DIVUI_OK);
  drain(document);
  assert(divui_element_perform_accessibility_action(viewport, DIVUI_ACCESSIBILITY_SCROLL_DOWN,
                                                    text(""), 0, 0) == DIVUI_OK);
  assert(divui_element_perform_accessibility_action(viewport, DIVUI_ACCESSIBILITY_SCROLL_RIGHT,
                                                    text(""), 0, 0) == DIVUI_OK);
  offset(viewport, 200.0, 200.0);
  drain(document);
  unsigned before_clamped = observer.calls;
  assert(divui_element_perform_accessibility_action(viewport, DIVUI_ACCESSIBILITY_SCROLL_DOWN,
                                                    text(""), 0, 0) == DIVUI_OK);
  assert(divui_element_perform_accessibility_action(viewport, DIVUI_ACCESSIBILITY_SCROLL_RIGHT,
                                                    text(""), 0, 0) == DIVUI_OK);
  drain(document);
  assert(observer.calls == before_clamped);

  DivUiAccessibilitySnapshot* second = NULL;
  assert(divui_document_accessibility_snapshot(document, first, &second) == DIVUI_OK);
  assert(divui_accessibility_snapshot_get_info(second, &info) == DIVUI_OK);
  assert(!info.full_tree && info.changed_count > 0);
  DivUiAccessibilityNodeState state;
  memset(&state, 0, sizeof(state));
  state.struct_size = sizeof(state);
  state.abi_version = DIVUI_ABI_VERSION;
  assert(divui_accessibility_snapshot_get_node_state(first, id, &state) == DIVUI_OK);
  assert(state.scroll_x == 0.0 && state.scroll_y == 0.0);
  assert(divui_accessibility_snapshot_get_node_state(second, id, &state) == DIVUI_OK);
  assert(state.scroll_x == 200.0 && state.scroll_y == 200.0);

  assert(divui_listener_destroy(listener) == DIVUI_OK);
  assert(divui_element_remove(viewport) == DIVUI_OK);
  assert(divui_element_destroy(content) == DIVUI_OK);
  assert(divui_element_destroy(viewport) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(document) == DIVUI_OK);
  assert(divui_accessibility_snapshot_get_node_state(second, id, &state) == DIVUI_OK);
  assert(state.scroll_x == 200.0 && state.scroll_y == 200.0);
  assert(divui_accessibility_snapshot_destroy(second) == DIVUI_OK);
  assert(divui_accessibility_snapshot_destroy(first) == DIVUI_OK);
  printf("native accessibility scroll: scale=%g actions=4 owned snapshots and callbacks passed\n",
         scale);
}

int main(void) {
  const double scales[] = {1.0, 1.25, 1.5, 1.75, 2.0};
  for (unsigned index = 0; index < sizeof(scales) / sizeof(scales[0]); ++index)
    exercise(scales[index]);
  return 0;
}
