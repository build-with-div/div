/* Keep public consuming checks active in packaged release builds. */
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "divui.h"
#include "portable_thread.h"

#if defined(__cplusplus)
static_assert(sizeof(DivUiAccessibilityScrollInfoV1) == 64, "append-only scroll info layout");
#else
_Static_assert(sizeof(DivUiAccessibilityScrollInfoV1) == 64, "append-only scroll info layout");
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
static DivUiAccessibilityScrollInfoV1 info(const DivUiAccessibilitySnapshot* snapshot,
                                           uint64_t id) {
  DivUiAccessibilityScrollInfoV1 result;
  memset(&result, 0, sizeof(result));
  result.struct_size = sizeof(result);
  result.abi_version = DIVUI_ABI_VERSION;
  assert(divui_accessibility_snapshot_get_scroll_info_v1(snapshot, id, &result) == DIVUI_OK);
  return result;
}
static DivUiAccessibilitySnapshot* capture(DivUiDocument* document,
                                           const DivUiAccessibilitySnapshot* previous) {
  DivUiAccessibilitySnapshot* result = NULL;
  assert(divui_document_accessibility_snapshot(document, previous, &result) == DIVUI_OK);
  return result;
}
static uint32_t all_flags(void) {
  return DIVUI_ACCESSIBILITY_SCROLL_HAS_X | DIVUI_ACCESSIBILITY_SCROLL_HAS_Y |
         DIVUI_ACCESSIBILITY_SCROLL_HAS_X_MIN | DIVUI_ACCESSIBILITY_SCROLL_HAS_X_MAX |
         DIVUI_ACCESSIBILITY_SCROLL_HAS_Y_MIN | DIVUI_ACCESSIBILITY_SCROLL_HAS_Y_MAX;
}
typedef struct ForeignQuery {
  const DivUiAccessibilitySnapshot* snapshot;
  uint64_t id;
} ForeignQuery;
static void* foreign_query(void* pointer) {
  ForeignQuery* query = (ForeignQuery*)pointer;
  DivUiAccessibilityScrollInfoV1 result;
  memset(&result, 0xa5, sizeof(result));
  result.struct_size = sizeof(result);
  result.abi_version = DIVUI_ABI_VERSION;
  const DivUiAccessibilityScrollInfoV1 before = result;
  assert(divui_accessibility_snapshot_get_scroll_info_v1(query->snapshot, query->id, &result) ==
         DIVUI_ERROR_WRONG_THREAD);
  assert(memcmp(&result, &before, sizeof(result)) == 0);
  return NULL;
}
static void exercise(double scale) {
  DivUiDocumentConfig config = {sizeof(config),
                                DIVUI_ABI_VERSION,
                                {320.0, 240.0, (uint32_t)(320.0 * scale), (uint32_t)(240.0 * scale),
                                 scale, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  DivUiElement *root = NULL, *scroll = NULL, *child = NULL;
  assert(divui_document_create(&config, &document) == DIVUI_OK);
  assert(divui_document_root(document, &root) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &scroll) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &child) == DIVUI_OK);
  assert(divui_element_append_child(root, scroll) == DIVUI_OK);
  assert(divui_element_append_child(scroll, child) == DIVUI_OK);
  property(scroll, DIVUI_STYLE_PROPERTY_WIDTH, "100px");
  property(scroll, DIVUI_STYLE_PROPERTY_HEIGHT, "80px");
  property(scroll, DIVUI_STYLE_PROPERTY_OVERFLOW, "auto");
  property(scroll, DIVUI_STYLE_PROPERTY_SCROLLBAR_WIDTH, "none");
  property(child, DIVUI_STYLE_PROPERTY_WIDTH, "160px");
  property(child, DIVUI_STYLE_PROPERTY_HEIGHT, "240px");
  uint64_t id = 0;
  assert(divui_element_get_accessibility_id(scroll, &id) == DIVUI_OK);
  DivUiAccessibilitySnapshot* initial = capture(document, NULL);
  DivUiAccessibilityScrollInfoV1 result = info(initial, id);
  assert(result.flags == all_flags());
  assert(result.actions == DIVUI_ACCESSIBILITY_SCROLL_DIRECTION_ACTION_MASK);
  assert(result.scroll_x == 0.0 && result.scroll_y == 0.0);
  assert(result.scroll_x_min == 0.0 && result.scroll_x_max == 60.0);
  assert(result.scroll_y_min == 0.0 && result.scroll_y_max == 160.0);
  assert(divui_element_scroll_to(scroll, 20.0, 40.0) == DIVUI_OK);
  property(scroll, DIVUI_STYLE_PROPERTY_OVERFLOW, "hidden");
  DivUiAccessibilitySnapshot* hidden = capture(document, initial);
  result = info(hidden, id);
  assert(result.flags == all_flags() && result.actions == 0);
  assert(result.scroll_x == 20.0 && result.scroll_y == 40.0);
  assert(result.scroll_x_min == 0.0 && result.scroll_x_max == 60.0);
  assert(result.scroll_y_min == 0.0 && result.scroll_y_max == 160.0);
  property(scroll, DIVUI_STYLE_PROPERTY_OVERFLOW, "clip");
  DivUiAccessibilitySnapshot* clipped = capture(document, hidden);
  result = info(clipped, id);
  assert(result.flags == 0 && result.actions == 0);
  assert(result.scroll_x == 0.0 && result.scroll_y == 0.0);
  assert(result.scroll_x_min == 0.0 && result.scroll_x_max == 0.0);
  assert(result.scroll_y_min == 0.0 && result.scroll_y_max == 0.0);
  property(scroll, DIVUI_STYLE_PROPERTY_DIRECTION, "rtl");
  property(scroll, DIVUI_STYLE_PROPERTY_OVERFLOW, "auto");
  assert(divui_element_scroll_to(scroll, -20.0, 40.0) == DIVUI_OK);
  DivUiAccessibilitySnapshot* rtl = capture(document, clipped);
  result = info(rtl, id);
  assert(result.flags == all_flags());
  assert(result.actions == DIVUI_ACCESSIBILITY_SCROLL_DIRECTION_ACTION_MASK);
  assert(result.scroll_x == -20.0 && result.scroll_y == 40.0);
  assert(result.scroll_x_min == -60.0 && result.scroll_x_max == 0.0);

  struct FutureInfo {
    DivUiAccessibilityScrollInfoV1 info;
    uint64_t tail[2];
  } future;
  memset(&future, 0, sizeof(future));
  future.info.struct_size = sizeof(future);
  future.info.abi_version = DIVUI_ABI_VERSION;
  future.tail[0] = UINT64_C(0x0123456789abcdef);
  future.tail[1] = UINT64_C(0xfedcba9876543210);
  assert(divui_accessibility_snapshot_get_scroll_info_v1(rtl, id, &future.info) == DIVUI_OK);
  assert(future.info.struct_size == sizeof(future));
  assert(future.tail[0] == UINT64_C(0x0123456789abcdef));
  assert(future.tail[1] == UINT64_C(0xfedcba9876543210));
  uint32_t short_output = sizeof(short_output);
  assert(divui_accessibility_snapshot_get_scroll_info_v1(
             rtl, id, (DivUiAccessibilityScrollInfoV1*)&short_output) ==
         DIVUI_ERROR_INVALID_ARGUMENT);
  assert(short_output == sizeof(short_output));
  uint32_t header[2] = {sizeof(header), DIVUI_ABI_VERSION};
  assert(divui_accessibility_snapshot_get_scroll_info_v1(
             rtl, id, (DivUiAccessibilityScrollInfoV1*)header) == DIVUI_ERROR_INVALID_ARGUMENT);
  assert(header[0] == sizeof(header) && header[1] == DIVUI_ABI_VERSION);
  memset(&result, 0xa5, sizeof(result));
  result.struct_size = sizeof(result);
  result.abi_version = DIVUI_ABI_VERSION;
  const DivUiAccessibilityScrollInfoV1 before = result;
  assert(divui_accessibility_snapshot_get_scroll_info_v1(rtl, UINT64_MAX, &result) ==
         DIVUI_ERROR_INVALID_ARGUMENT);
  assert(memcmp(&result, &before, sizeof(result)) == 0);
  ForeignQuery query = {rtl, id};
  pthread_t thread;
  assert(pthread_create(&thread, NULL, foreign_query, &query) == 0);
  assert(pthread_join(thread, NULL) == 0);

  assert(divui_element_destroy(child) == DIVUI_OK);
  assert(divui_element_destroy(scroll) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(document) == DIVUI_OK);
  /* Every value below comes from the owned tree after the live document died. */
  assert(info(initial, id).scroll_x == 0.0);
  assert(info(hidden, id).scroll_y == 40.0);
  assert(info(clipped, id).flags == 0);
  assert(info(rtl, id).scroll_x_min == -60.0);
  assert(divui_accessibility_snapshot_destroy(initial) == DIVUI_OK);
  assert(divui_accessibility_snapshot_destroy(hidden) == DIVUI_OK);
  assert(divui_accessibility_snapshot_destroy(clipped) == DIVUI_OK);
  assert(divui_accessibility_snapshot_destroy(rtl) == DIVUI_OK);
  assert(divui_accessibility_snapshot_get_scroll_info_v1(rtl, id, &result) ==
         DIVUI_ERROR_INVALID_HANDLE);
  assert(memcmp(&result, &before, sizeof(result)) == 0);
  printf("owned accessibility scroll info: scale=%g positions/ranges/actions/lifetimes passed\n",
         scale);
}
int main(void) {
  const double scales[] = {1.0, 1.25, 1.5, 1.75, 2.0};
  for (unsigned i = 0; i < sizeof(scales) / sizeof(scales[0]); ++i)
    exercise(scales[i]);
  return 0;
}
