/* Keep consumer checks active when the SDK is built with NDEBUG. */
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "divui.h"

static DivUiUtf8 utf8(const char* value) {
  DivUiUtf8 result = {(const uint8_t*)value, strlen(value)};
  return result;
}

static void property(DivUiElement* element, DivUiStyleProperty id, const char* literal) {
  DivUiStyleValue value;
  DivUiStatus status = divui_style_value_parse(id, utf8(literal), &value);
  if (status != DIVUI_OK)
    fprintf(stderr, "geometry property %d literal '%s': %d\n", id, literal, status);
  assert(status == DIVUI_OK);
  assert(divui_element_set_property(element, id, &value) == DIVUI_OK);
  if (value.tag == DIVUI_STYLE_VALUE_COMPOUND)
    assert(divui_style_compound_destroy((DivUiStyleCompound*)value.data.compound) == DIVUI_OK);
}

static DivUiElement* block(DivUiDocument* document, DivUiElement* parent) {
  DivUiElement* element = NULL;
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &element) == DIVUI_OK);
  property(element, DIVUI_STYLE_PROPERTY_DISPLAY, "block");
  assert(divui_element_append_child(parent, element) == DIVUI_OK);
  return element;
}

static void vertical_geometry(const char* writing_mode, float final_parent_x, float hit_x) {
  DivUiDocumentConfig config = {
      sizeof(config), DIVUI_ABI_VERSION, {320.0, 240.0, 320, 240, 1.0, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  DivUiElement* root = NULL;
  assert(divui_document_create(&config, &document) == DIVUI_OK);
  assert(divui_document_root(document, &root) == DIVUI_OK);
  DivUiElement* columns = block(document, root);
  property(columns, DIVUI_STYLE_PROPERTY_HEIGHT, "300px");
  property(columns, DIVUI_STYLE_PROPERTY_WRITING_MODE, writing_mode);
  property(columns, DIVUI_STYLE_PROPERTY_COLUMN_COUNT, "3");
  property(columns, DIVUI_STYLE_PROPERTY_COLUMN_GAP, "24px");
  DivUiElement* wrapper = block(document, columns);
  property(wrapper, DIVUI_STYLE_PROPERTY_WRITING_MODE, writing_mode);
  property(wrapper, DIVUI_STYLE_PROPERTY_MAX_WIDTH, "160px");
  DivUiElement* target = block(document, wrapper);
  property(target, DIVUI_STYLE_PROPERTY_WIDTH, "200px");
  property(target, DIVUI_STYLE_PROPERTY_HEIGHT, "50px");
  property(target, DIVUI_STYLE_PROPERTY_BORDER, "3px solid black");
  DivUiRect bounds;
  assert(divui_element_get_bounds(columns, &bounds) == DIVUI_OK && bounds.width == 68.671875f);
  size_t count = 0;
  DivUiRect wrapper_rects[3];
  assert(divui_element_get_client_rects_v1(wrapper, wrapper_rects, 3, &count) == DIVUI_OK &&
         count == 3);
  assert(wrapper_rects[0].y == 0.0f && wrapper_rects[1].y == 108.0f &&
         wrapper_rects[2].y == 216.0f);
  assert(wrapper_rects[0].width == 68.671875f && wrapper_rects[1].width == 68.671875f);
  assert(wrapper_rects[2].width == 22.65625f && wrapper_rects[2].x == final_parent_x);
  assert(wrapper_rects[0].height == 84.0f && wrapper_rects[1].height == 84.0f &&
         wrapper_rects[2].height == 84.0f);
  DivUiRect rects[3];
  assert(divui_element_get_client_rects_v1(target, rects, 3, &count) == DIVUI_OK && count == 3);
  assert(rects[0].y == 0.0f && rects[1].y == 108.0f && rects[2].y == 216.0f);
  assert(rects[0].width == 68.671875f && rects[1].width == 68.671875f &&
         rects[2].width == 68.65625f);
  assert(rects[0].height == 56.0f && rects[1].height == 56.0f && rects[2].height == 56.0f);
  DivUiElement* hit = NULL;
  assert(divui_document_hit_test(document, hit_x, 230.0f, &hit) == DIVUI_OK && hit != NULL);
  assert(hit == target); /* Hit testing borrows the existing native handle. */
  DivUiRect hit_rects[3];
  assert(divui_element_get_client_rects_v1(hit, hit_rects, 3, &count) == DIVUI_OK && count == 3);
  for (size_t i = 0; i < count; ++i) {
    assert(hit_rects[i].x == rects[i].x && hit_rects[i].y == rects[i].y &&
           hit_rects[i].width == rects[i].width && hit_rects[i].height == rects[i].height);
  }
  assert(divui_element_destroy(target) == DIVUI_OK);
  assert(divui_element_destroy(wrapper) == DIVUI_OK);
  assert(divui_element_destroy(columns) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(document) == DIVUI_OK);
}

int main(void) {
  DivUiDocumentConfig config = {
      sizeof(config), DIVUI_ABI_VERSION, {320.0, 240.0, 320, 240, 1.0, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  DivUiElement* root = NULL;
  assert(divui_document_create(&config, &document) == DIVUI_OK);
  assert(divui_document_root(document, &root) == DIVUI_OK);
  DivUiStyleValue invalid_count;
  assert(divui_style_value_parse(DIVUI_STYLE_PROPERTY_COLUMN_COUNT, utf8("auto"), &invalid_count) ==
         DIVUI_OK);
  assert(invalid_count.tag == DIVUI_STYLE_VALUE_INTEGER && invalid_count.data.integer == 0);
  assert(divui_style_value_parse(DIVUI_STYLE_PROPERTY_COLUMN_COUNT, utf8("0"), &invalid_count) ==
         DIVUI_ERROR_INVALID_ARGUMENT);
  assert(divui_style_value_parse(DIVUI_STYLE_PROPERTY_COLUMN_COUNT, utf8("-1"), &invalid_count) ==
         DIVUI_ERROR_INVALID_ARGUMENT);
  assert(divui_style_value_parse(DIVUI_STYLE_PROPERTY_COLUMN_COUNT, utf8("4294967296"),
                                 &invalid_count) == DIVUI_ERROR_INVALID_ARGUMENT);
  DivUiElement* columns = block(document, root);
  property(columns, DIVUI_STYLE_PROPERTY_WIDTH, "300px");
  property(columns, DIVUI_STYLE_PROPERTY_COLUMN_COUNT, "3");
  property(columns, DIVUI_STYLE_PROPERTY_COLUMN_GAP, "24px");
  DivUiElement* wrapper = block(document, columns);
  property(wrapper, DIVUI_STYLE_PROPERTY_MAX_HEIGHT, "160px");
  DivUiElement* target = block(document, wrapper);
  property(target, DIVUI_STYLE_PROPERTY_WIDTH, "50px");
  property(target, DIVUI_STYLE_PROPERTY_HEIGHT, "200px");
  property(target, DIVUI_STYLE_PROPERTY_BORDER, "3px solid black");

  size_t count = 0;
  assert(divui_element_get_client_rects_v1(target, NULL, 0, &count) == DIVUI_OK);
  assert(count == 3);
  DivUiRect rects[3] = {{-1.0f, -1.0f, -1.0f, -1.0f}};
  assert(divui_element_get_client_rects_v1(target, rects, 2, &count) ==
         DIVUI_ERROR_BUFFER_TOO_SMALL);
  assert(count == 3 && rects[0].x == -1.0f);
  assert(divui_element_get_client_rects_v1(target, rects, 3, &count) == DIVUI_OK);
  assert(rects[0].x == 0.0f && rects[1].x == 108.0f && rects[2].x == 216.0f);
  assert(rects[0].height == 68.671875f && rects[2].height == 68.65625f);
  DivUiRect bounds;
  assert(divui_element_get_bounds(target, &bounds) == DIVUI_OK);
  assert(bounds.x == 0.0f && bounds.y == 0.0f && bounds.width == 272.0f);
  assert(bounds.height == 68.671875f);
  DivUiRect wrapper_rects[3];
  assert(divui_element_get_client_rects_v1(wrapper, wrapper_rects, 3, &count) == DIVUI_OK);
  assert(count == 3 && wrapper_rects[2].width == 84.0f);
  assert(wrapper_rects[2].height == 22.65625f);
  property(wrapper, DIVUI_STYLE_PROPERTY_MAX_HEIGHT, "120px");
  assert(divui_element_get_client_rects_v1(wrapper, rects, 3, &count) == DIVUI_OK);
  assert(rects[1].height == 51.328125f && rects[2].height == 0.0f);
  assert(divui_element_get_bounds(wrapper, &bounds) == DIVUI_OK);
  assert(bounds.width == 192.0f && bounds.height == 68.671875f);
  assert(wrapper_rects[2].height == 22.65625f); /* Earlier copies remain owned. */
  assert(divui_element_get_client_rects_v1(target, rects, 3, &count) == DIVUI_OK);
  assert(rects[2].height == 68.65625f); /* The child still owns visible overflow. */
  property(wrapper, DIVUI_STYLE_PROPERTY_MAX_HEIGHT, "0px");
  assert(divui_element_get_client_rects_v1(wrapper, rects, 3, &count) == DIVUI_OK && count == 3);
  for (size_t part = 0; part < count; ++part) {
    assert(rects[part].x == 108.0f * part && rects[part].y == 0.0f);
    assert(rects[part].width == 84.0f && rects[part].height == 0.0f);
  }
  assert(divui_element_get_bounds(wrapper, &bounds) == DIVUI_OK);
  assert(bounds.x == 216.0f && bounds.y == 0.0f && bounds.width == 84.0f && bounds.height == 0.0f);
  assert(wrapper_rects[2].height == 22.65625f); /* Earlier owned copies remain unchanged. */
  property(wrapper, DIVUI_STYLE_PROPERTY_MAX_HEIGHT, "160px");
  /* An independent block formatting context still fragments ordinary children. */
  property(wrapper, DIVUI_STYLE_PROPERTY_DISPLAY, "flow-root");
  assert(divui_element_get_client_rects_v1(target, rects, 3, &count) == DIVUI_OK && count == 3);
  assert(rects[0].y == 0.0f && rects[1].y == 0.0f && rects[2].y == 0.0f);
  assert(rects[0].height == 68.671875f && rects[1].height == 68.671875f &&
         rects[2].height == 68.65625f);
  assert(divui_element_get_bounds(target, &bounds) == DIVUI_OK);
  assert(bounds.y == 0.0f && bounds.width == 272.0f && bounds.height == 68.671875f);
  /* Constrained column flex owns its box independently of item overflow. */
  property(wrapper, DIVUI_STYLE_PROPERTY_DISPLAY, "flex");
  property(wrapper, DIVUI_STYLE_PROPERTY_FLEX_DIRECTION, "column");
  property(target, DIVUI_STYLE_PROPERTY_FLEX_SHRINK, "0");
  property(target, DIVUI_STYLE_PROPERTY_FLEX_BASIS, "auto");
  assert(divui_element_get_client_rects_v1(target, rects, 3, &count) == DIVUI_OK && count == 3);
  assert(rects[0].y == 0.0f && rects[1].y == 0.0f && rects[2].y == 0.0f);
  assert(rects[0].height == 68.671875f && rects[1].height == 68.671875f &&
         rects[2].height == 68.65625f);
  assert(divui_element_get_client_rects_v1(wrapper, wrapper_rects, 3, &count) == DIVUI_OK &&
         count == 3 && wrapper_rects[2].height == 22.65625f);
  DivUiElement* flex_hit = NULL;
  assert(divui_document_hit_test(document, 240.0f, 40.0f, &flex_hit) == DIVUI_OK &&
         flex_hit == target);
  property(wrapper, DIVUI_STYLE_PROPERTY_MAX_HEIGHT, "120px");
  assert(divui_element_get_client_rects_v1(wrapper, rects, 3, &count) == DIVUI_OK && count == 3);
  assert(rects[1].height == 51.328125f && rects[2].height == 0.0f);
  assert(wrapper_rects[2].height == 22.65625f); /* The earlier snapshot remains owned. */
  property(wrapper, DIVUI_STYLE_PROPERTY_MAX_HEIGHT, "160px");
  /* Atomic flex content remains above the following ordinary block background. */
  DivUiElement* following = block(document, columns);
  property(following, DIVUI_STYLE_PROPERTY_WIDTH, "20px");
  property(following, DIVUI_STYLE_PROPERTY_HEIGHT, "20px");
  property(following, DIVUI_STYLE_PROPERTY_BACKGROUND_COLOR, "blue");
  assert(divui_document_hit_test(document, 227.0f, 30.0f, &flex_hit) == DIVUI_OK &&
         flex_hit == target);
  property(following, DIVUI_STYLE_PROPERTY_POSITION, "relative");
  assert(divui_document_hit_test(document, 227.0f, 30.0f, &flex_hit) == DIVUI_OK &&
         flex_hit == following);
  property(following, DIVUI_STYLE_PROPERTY_POSITION, "static");
  assert(divui_document_hit_test(document, 227.0f, 30.0f, &flex_hit) == DIVUI_OK &&
         flex_hit == target);
  property(following, DIVUI_STYLE_PROPERTY_OPACITY, "0.5");
  assert(divui_document_hit_test(document, 227.0f, 30.0f, &flex_hit) == DIVUI_OK &&
         flex_hit == following);
  property(following, DIVUI_STYLE_PROPERTY_OPACITY, "1");
  assert(divui_document_hit_test(document, 227.0f, 30.0f, &flex_hit) == DIVUI_OK &&
         flex_hit == target);
  assert(divui_element_remove(following) == DIVUI_OK);
  assert(divui_element_destroy(following) == DIVUI_OK);
  property(wrapper, DIVUI_STYLE_PROPERTY_DISPLAY, "block");
  property(target, DIVUI_STYLE_PROPERTY_POINTER_EVENTS, "none");
  property(target, DIVUI_STYLE_PROPERTY_VISIBILITY, "hidden");
  assert(divui_element_get_bounds(target, &bounds) == DIVUI_OK && bounds.width == 272.0f);
  assert(divui_element_get_client_rects_v1(target, NULL, 0, &count) == DIVUI_OK && count == 3);
  assert(divui_element_get_client_rects_v1(target, rects, 3, NULL) == DIVUI_ERROR_INVALID_ARGUMENT);

  assert(divui_element_detach(target) == DIVUI_OK);
  assert(divui_element_get_client_rects_v1(target, NULL, 0, &count) == DIVUI_OK && count == 0);
  assert(rects[2].x == 216.0f); /* Copied values remain owned by this consumer. */
  assert(divui_element_append_child(wrapper, target) == DIVUI_OK);
  assert(divui_element_get_client_rects_v1(target, NULL, 0, &count) == DIVUI_OK && count == 3);
  /* Native C mutations reach the same deferred atomic-child layout as Rust. */
  property(target, DIVUI_STYLE_PROPERTY_POINTER_EVENTS, "auto");
  property(target, DIVUI_STYLE_PROPERTY_VISIBILITY, "visible");
  property(columns, DIVUI_STYLE_PROPERTY_MAX_HEIGHT, "80px");
  DivUiElement* clipped = block(document, target);
  property(clipped, DIVUI_STYLE_PROPERTY_WIDTH, "20px");
  property(clipped, DIVUI_STYLE_PROPERTY_OVERFLOW, "hidden");
  const char* atomic_heights[] = {"90px", "200px", "300px"};
  const float child_heights[][3] = {
      {80.0f, 90.0f, 36.0f}, {80.0f, 126.0f, 0.0f}, {80.0f, 126.0f, 0.0f}};
  const size_t fragment_counts[] = {3, 2, 2};
  for (size_t state = 0; state < 3; ++state) {
    property(clipped, DIVUI_STYLE_PROPERTY_HEIGHT, atomic_heights[state]);
    assert(divui_element_get_client_rects_v1(target, rects, 3, &count) == DIVUI_OK);
    assert(count == fragment_counts[state]);
    for (size_t part = 0; part < count; ++part) {
      assert(rects[part].x == 108.0f * part && rects[part].y == 0.0f);
      assert(rects[part].height == child_heights[state][part]);
    }
    assert(divui_element_get_client_rects_v1(wrapper, wrapper_rects, 3, &count) == DIVUI_OK);
    assert(count == fragment_counts[state]);
    assert(wrapper_rects[0].height == 80.0f && wrapper_rects[1].height == 80.0f);
    if (count == 3)
      assert(wrapper_rects[2].height == 0.0f);
  }
  property(wrapper, DIVUI_STYLE_PROPERTY_PADDING_TOP, "10px");
  property(clipped, DIVUI_STYLE_PROPERTY_HEIGHT, "200px");
  assert(divui_element_get_client_rects_v1(target, rects, 3, &count) == DIVUI_OK && count == 2);
  assert(rects[0].y == 10.0f && rects[0].height == 70.0f);
  assert(rects[1].y == 0.0f && rects[1].height == 136.0f);
  assert(divui_element_get_client_rects_v1(wrapper, wrapper_rects, 3, &count) == DIVUI_OK);
  assert(count == 2 && wrapper_rects[0].height == 80.0f && wrapper_rects[1].height == 90.0f);
  assert(divui_element_destroy(clipped) == DIVUI_OK);
  assert(divui_element_remove(target) == DIVUI_OK);
  assert(divui_element_get_client_rects_v1(target, NULL, 0, &count) == DIVUI_ERROR_STALE_HANDLE);
  assert(divui_element_destroy(target) == DIVUI_OK);
  assert(divui_element_destroy(wrapper) == DIVUI_OK);
  assert(divui_element_destroy(columns) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(document) == DIVUI_OK);
  vertical_geometry("vertical-lr", 0.0f, 40.0f);
  vertical_geometry("vertical-rl", 46.015625f, 20.0f);
  puts("geometry: native column boxes, child overflow, owned copies, bounds and lifecycle passed");
  return 0;
}
