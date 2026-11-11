/* Public C/C++ consumers of retained native scrollbar and overflow geometry.
 * Run after the shared layout implementation and runtime ABI gate are built.
 * Logical dimensions must be independent of raster scale. */
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "divui.h"

static DivUiUtf8 utf8(const char* text) {
  DivUiUtf8 result = {(const uint8_t*)text, strlen(text)};
  return result;
}

static void property(DivUiElement* element, DivUiStyleProperty id, const char* literal) {
  DivUiStyleValue value;
  DivUiStatus status = divui_style_value_parse(id, utf8(literal), &value);
  if (status != DIVUI_OK)
    fprintf(stderr, "native gutter property %d '%s': %d\n", (int)id, literal, (int)status);
  assert(status == DIVUI_OK);
  assert(divui_element_set_property(element, id, &value) == DIVUI_OK);
  if (value.tag == DIVUI_STYLE_VALUE_COMPOUND)
    assert(divui_style_compound_destroy((DivUiStyleCompound*)value.data.compound) == DIVUI_OK);
}

typedef DivUiStatus (*DimensionQuery)(DivUiElement*, int32_t*);

static void dimensions(DivUiElement* element,
                       int32_t width,
                       int32_t height,
                       int32_t scroll_width,
                       int32_t scroll_height) {
  const DimensionQuery queries[] = {
      divui_element_get_client_width_v1, divui_element_get_client_height_v1,
      divui_element_get_scroll_width_v1, divui_element_get_scroll_height_v1};
  const int32_t expected[] = {width, height, scroll_width, scroll_height};
  for (size_t i = 0; i < sizeof(queries) / sizeof(queries[0]); ++i) {
    struct Output {
      int32_t value;
      uint8_t tail[12];
    } output;
    output.value = INT32_MIN;
    memset(output.tail, 0x6d, sizeof(output.tail));
    assert(queries[i](element, &output.value) == DIVUI_OK);
    if (output.value != expected[i])
      fprintf(stderr, "native gutter dimension %zu: got %d expected %d\n", i, (int)output.value,
              (int)expected[i]);
    assert(output.value == expected[i]);
    for (size_t j = 0; j < sizeof(output.tail); ++j)
      assert(output.tail[j] == 0x6d);
  }
}

static DivUiRect bounds(DivUiElement* element) {
  DivUiRect value;
  assert(divui_element_get_bounds(element, &value) == DIVUI_OK);
  return value;
}

static void size(DivUiElement* element, float width, float height) {
  DivUiRect value = bounds(element);
  assert(value.width == width && value.height == height);
}

static DivUiScrollMetricsV1 raw_metrics(DivUiElement* element) {
  DivUiScrollMetricsV1 value = {sizeof(value), DIVUI_ABI_VERSION, 0.0, 0.0, 0.0, 0.0};
  uint8_t found = 0;
  assert(divui_element_get_scroll_metrics_v1(element, &value, &found) == DIVUI_OK);
  assert(found == 1);
  return value;
}

static void offset(DivUiElement* element, double expected_x, double expected_y) {
  double x = -999.0, y = -999.0;
  assert(divui_element_get_scroll_offset(element, &x, &y) == DIVUI_OK);
  assert(x == expected_x && y == expected_y);
}

static void consumer(const char* display, double scale) {
  DivUiDocumentConfig config = {sizeof(config),
                                DIVUI_ABI_VERSION,
                                {320.0, 240.0, (uint32_t)(320.0 * scale), (uint32_t)(240.0 * scale),
                                 scale, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  DivUiElement *root = NULL, *container = NULL, *child = NULL, *absolute = NULL;
  assert(divui_document_create(&config, &document) == DIVUI_OK);
  assert(divui_document_root(document, &root) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &container) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &child) == DIVUI_OK);
  assert(divui_element_append_child(root, container) == DIVUI_OK);
  assert(divui_element_append_child(container, child) == DIVUI_OK);
  property(container, DIVUI_STYLE_PROPERTY_DISPLAY, display);
  property(container, DIVUI_STYLE_PROPERTY_POSITION, "absolute");
  property(container, DIVUI_STYLE_PROPERTY_LEFT, "20px");
  property(container, DIVUI_STYLE_PROPERTY_TOP, "20px");
  property(container, DIVUI_STYLE_PROPERTY_WIDTH, "100px");
  property(container, DIVUI_STYLE_PROPERTY_HEIGHT, "80px");
  property(container, DIVUI_STYLE_PROPERTY_BOX_SIZING, "content-box");
  property(container, DIVUI_STYLE_PROPERTY_OVERFLOW, "scroll");
  property(container, DIVUI_STYLE_PROPERTY_FLEX_DIRECTION, "column");
  property(container, DIVUI_STYLE_PROPERTY_ALIGN_ITEMS, "start");
  property(container, DIVUI_STYLE_PROPERTY_GRID_TEMPLATE_COLUMNS, "100%");
  property(container, DIVUI_STYLE_PROPERTY_GRID_TEMPLATE_ROWS, "100%");
  property(child, DIVUI_STYLE_PROPERTY_WIDTH, "100%");
  property(child, DIVUI_STYLE_PROPERTY_HEIGHT, "100%");
  property(child, DIVUI_STYLE_PROPERTY_MIN_WIDTH, "0px");
  property(child, DIVUI_STYLE_PROPERTY_MIN_HEIGHT, "0px");
  property(child, DIVUI_STYLE_PROPERTY_FLEX_SHRINK, "0");
  dimensions(container, 85, 65, 85, 65);
  size(container, 100.0f, 80.0f);
  size(child, 85.0f, 65.0f);

  property(container, DIVUI_STYLE_PROPERTY_SCROLLBAR_WIDTH, "none");
  dimensions(container, 100, 80, 100, 80);
  size(child, 100.0f, 80.0f);
  property(container, DIVUI_STYLE_PROPERTY_SCROLLBAR_WIDTH, "thin");
  dimensions(container, 90, 70, 90, 70);
  size(child, 90.0f, 70.0f);
  property(container, DIVUI_STYLE_PROPERTY_SCROLLBAR_WIDTH, "auto");
  property(container, DIVUI_STYLE_PROPERTY_PADDING, "5px");
  property(container, DIVUI_STYLE_PROPERTY_BORDER, "2px solid black");
  dimensions(container, 95, 75, 95, 75);
  size(container, 114.0f, 94.0f);
  size(child, 85.0f, 65.0f);

  /* Retain fractional layout metrics separately from the integer DOM-style API.
   */
  property(container, DIVUI_STYLE_PROPERTY_PADDING, "0px");
  property(container, DIVUI_STYLE_PROPERTY_BORDER, "0px solid black");
  property(container, DIVUI_STYLE_PROPERTY_WIDTH, "100.25px");
  property(container, DIVUI_STYLE_PROPERTY_HEIGHT, "80.5px");
  dimensions(container, 85, 66, 85, 66);
  DivUiScrollMetricsV1 fractional = raw_metrics(container);
  assert(fractional.client_width == 85.25 && fractional.client_height == 65.5);
  size(child, 85.25f, 65.5f);
  DivUiTransformOperation operations[2];
  memset(operations, 0, sizeof(operations));
  operations[0].kind = 1;
  operations[0].x = (DivUiLength){0.25f, 0};
  operations[0].y = (DivUiLength){0.5f, 0};
  operations[1].kind = 2;
  operations[1].values[0] = 2.0f;
  operations[1].values[1] = 2.0f;
  DivUiStyleCompound* transform = NULL;
  assert(divui_transform_create(operations, 2, &transform) == DIVUI_OK);
  DivUiStyleValue transform_value;
  memset(&transform_value, 0, sizeof(transform_value));
  transform_value.tag = DIVUI_STYLE_VALUE_COMPOUND;
  transform_value.data.compound = transform;
  assert(divui_element_set_property(container, DIVUI_STYLE_PROPERTY_TRANSFORM, &transform_value) ==
         DIVUI_OK);
  assert(divui_style_compound_destroy(transform) == DIVUI_OK);
  dimensions(container, 85, 66, 85, 66);
  property(container, DIVUI_STYLE_PROPERTY_TRANSFORM, "none");
  property(container, DIVUI_STYLE_PROPERTY_WIDTH, "100px");
  property(container, DIVUI_STYLE_PROPERTY_HEIGHT, "80px");

  /* Absolute percentages resolve against the same gutter-reduced containing
   * box. */
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &absolute) == DIVUI_OK);
  assert(divui_element_append_child(container, absolute) == DIVUI_OK);
  property(absolute, DIVUI_STYLE_PROPERTY_POSITION, "absolute");
  property(absolute, DIVUI_STYLE_PROPERTY_LEFT, "0px");
  property(absolute, DIVUI_STYLE_PROPERTY_TOP, "0px");
  property(absolute, DIVUI_STYLE_PROPERTY_WIDTH, "100%");
  property(absolute, DIVUI_STYLE_PROPERTY_HEIGHT, "100%");
  dimensions(container, 85, 65, 85, 65);
  size(absolute, 85.0f, 65.0f);
  assert(divui_element_detach(absolute) == DIVUI_OK);

  /* Auto overflow must relayout percentage descendants and clamp retained
   * offsets. */
  property(container, DIVUI_STYLE_PROPERTY_OVERFLOW, "auto");
  property(child, DIVUI_STYLE_PROPERTY_WIDTH, "150px");
  property(child, DIVUI_STYLE_PROPERTY_HEIGHT, "120px");
  dimensions(container, 85, 65, 150, 120);
  assert(divui_element_scroll_to(container, 1000.0, 1000.0) == DIVUI_OK);
  offset(container, 65.0, 55.0);
  property(child, DIVUI_STYLE_PROPERTY_WIDTH, "20px");
  property(child, DIVUI_STYLE_PROPERTY_HEIGHT, "20px");
  dimensions(container, 100, 80, 100, 80);
  offset(container, 0.0, 0.0);
  property(child, DIVUI_STYLE_PROPERTY_WIDTH, "100%");
  property(child, DIVUI_STYLE_PROPERTY_HEIGHT, "100%");
  dimensions(container, 100, 80, 100, 80);
  size(child, 100.0f, 80.0f);

  /* Stable gutters survive absent overflow and follow writing direction. */
  property(container, DIVUI_STYLE_PROPERTY_OVERFLOW, "hidden");
  property(container, DIVUI_STYLE_PROPERTY_DIRECTION, "rtl");
  property(container, DIVUI_STYLE_PROPERTY_SCROLLBAR_GUTTER, "stable");
  dimensions(container, 85, 80, 85, 80);
  size(child, 85.0f, 80.0f);
  assert(bounds(child).x == bounds(container).x + 15.0f);
  property(container, DIVUI_STYLE_PROPERTY_SCROLLBAR_GUTTER, "stable both-edges");
  dimensions(container, 70, 80, 70, 80);
  size(child, 70.0f, 80.0f);
  assert(bounds(child).x == bounds(container).x + 15.0f);
  property(container, DIVUI_STYLE_PROPERTY_DIRECTION, "ltr");
  property(container, DIVUI_STYLE_PROPERTY_WRITING_MODE, "vertical-rl");
  property(container, DIVUI_STYLE_PROPERTY_SCROLLBAR_GUTTER, "stable");
  dimensions(container, 100, 65, 100, 65);
  size(child, 100.0f, 65.0f);
  assert(bounds(child).y == bounds(container).y);
  property(container, DIVUI_STYLE_PROPERTY_SCROLLBAR_GUTTER, "stable both-edges");
  dimensions(container, 100, 50, 100, 50);
  size(child, 100.0f, 50.0f);
  assert(bounds(child).y == bounds(container).y + 15.0f);

  /* No-box transitions use the API's zero dimensions, then recover live
   * geometry. */
  property(container, DIVUI_STYLE_PROPERTY_DISPLAY, "none");
  dimensions(container, 0, 0, 0, 0);
  property(container, DIVUI_STYLE_PROPERTY_DISPLAY, display);
  dimensions(container, 100, 50, 100, 50);
  assert(divui_element_detach(container) == DIVUI_OK);
  dimensions(container, 0, 0, 0, 0);
  assert(divui_element_append_child(root, container) == DIVUI_OK);
  dimensions(container, 100, 50, 100, 50);
  assert(fractional.client_width == 85.25 && fractional.client_height == 65.5);

  assert(divui_element_destroy(absolute) == DIVUI_OK);
  assert(divui_element_destroy(child) == DIVUI_OK);
  assert(divui_element_destroy(container) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(document) == DIVUI_OK);
  printf("native C flex/grid scroll gutters: display=%s scale=%g passed\n", display, scale);
}

static DivUiDocument* document_for_scale(double scale, DivUiElement** root) {
  DivUiDocumentConfig config = {sizeof(config),
                                DIVUI_ABI_VERSION,
                                {320.0, 240.0, (uint32_t)(320.0 * scale), (uint32_t)(240.0 * scale),
                                 scale, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  assert(divui_document_create(&config, &document) == DIVUI_OK);
  assert(divui_document_root(document, root) == DIVUI_OK);
  return document;
}

static void authored_size(DivUiElement* element) {
  property(element, DIVUI_STYLE_PROPERTY_WIDTH, "100px");
  property(element, DIVUI_STYLE_PROPERTY_HEIGHT, "80px");
  property(element, DIVUI_STYLE_PROPERTY_BOX_SIZING, "content-box");
}

static void table_consumer(double scale) {
  DivUiElement *root = NULL, *table = NULL, *row = NULL, *cell = NULL;
  DivUiDocument* document = document_for_scale(scale, &root);
  assert(divui_element_create(document, DIVUI_ELEMENT_TABLE, &table) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_TABLE_ROW, &row) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_TABLE_CELL, &cell) == DIVUI_OK);
  assert(divui_element_append_child(root, table) == DIVUI_OK);
  assert(divui_element_append_child(table, row) == DIVUI_OK);
  assert(divui_element_append_child(row, cell) == DIVUI_OK);
  authored_size(table);
  property(table, DIVUI_STYLE_PROPERTY_BORDER_SPACING, "0px");
  property(cell, DIVUI_STYLE_PROPERTY_HEIGHT, "80px");
  property(table, DIVUI_STYLE_PROPERTY_SCROLLBAR_GUTTER, "stable");
  const char* overflow[] = {"auto", "scroll"};
  for (size_t i = 0; i < sizeof(overflow) / sizeof(overflow[0]); ++i) {
    property(table, DIVUI_STYLE_PROPERTY_OVERFLOW, overflow[i]);
    dimensions(table, 100, 80, 100, 80);
    assert(divui_element_scroll_to(table, 1000.0, 1000.0) == DIVUI_OK);
    offset(table, 0.0, 0.0);
  }
  /* Hidden/hidden keeps table scrolling; columns retain authored grid width. */
  property(table, DIVUI_STYLE_PROPERTY_OVERFLOW, "hidden");
  dimensions(table, 85, 80, 100, 80);
  dimensions(cell, 100, 80, 100, 80);
  size(table, 100.0f, 80.0f);
  assert(divui_element_scroll_to(table, 1000.0, 1000.0) == DIVUI_OK);
  offset(table, 15.0, 0.0);
  /* A mixed hidden/auto table normalizes both axes to visible. */
  property(table, DIVUI_STYLE_PROPERTY_OVERFLOW_Y, "auto");
  dimensions(table, 100, 80, 100, 80);
  offset(table, 0.0, 0.0);
  assert(divui_element_destroy(cell) == DIVUI_OK);
  assert(divui_element_destroy(row) == DIVUI_OK);
  assert(divui_element_destroy(table) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(document) == DIVUI_OK);
  printf("native C table scroll dimensions: scale=%g passed\n", scale);
}

static void replaced_consumer(double scale) {
  DivUiElement *root = NULL, *image = NULL;
  DivUiResource* resource = NULL;
  DivUiDocument* document = document_for_scale(scale, &root);
  assert(divui_element_create(document, DIVUI_ELEMENT_IMAGE, &image) == DIVUI_OK);
  /* A missing image uses block fallback layout. Install actual primary content.
   */
  const char svg[] =
      "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\">"
      "<rect width=\"1\" height=\"1\" fill=\"black\"/></svg>";
  assert(divui_document_register_image(
             document, utf8("memory:dimensions-pixel.svg"), utf8("image/svg+xml"),
             utf8("949c974a51bfed3daa2be2aed981026336857d4cfd494dd120dbb6da66c8"
                  "91f7"),
             (const uint8_t*)svg, sizeof(svg) - 1, &resource) == DIVUI_OK);
  assert(divui_element_set_image(image, resource, 1.0f, 1.0f) == DIVUI_OK);
  assert(divui_element_append_child(root, image) == DIVUI_OK);
  authored_size(image);
  property(image, DIVUI_STYLE_PROPERTY_OVERFLOW, "scroll");
  dimensions(image, 100, 80, 100, 80);
  property(image, DIVUI_STYLE_PROPERTY_OVERFLOW, "hidden");
  property(image, DIVUI_STYLE_PROPERTY_SCROLLBAR_GUTTER, "stable");
  dimensions(image, 85, 80, 85, 80);
  size(image, 100.0f, 80.0f);
  property(image, DIVUI_STYLE_PROPERTY_DIRECTION, "rtl");
  dimensions(image, 85, 80, 85, 80);
  property(image, DIVUI_STYLE_PROPERTY_SCROLLBAR_GUTTER, "stable both-edges");
  dimensions(image, 70, 80, 70, 80);
  assert(divui_element_scroll_to(image, -1000.0, 1000.0) == DIVUI_OK);
  offset(image, 0.0, 0.0);
  property(image, DIVUI_STYLE_PROPERTY_DIRECTION, "ltr");
  property(image, DIVUI_STYLE_PROPERTY_WRITING_MODE, "vertical-rl");
  property(image, DIVUI_STYLE_PROPERTY_SCROLLBAR_GUTTER, "stable");
  dimensions(image, 100, 65, 100, 65);
  property(image, DIVUI_STYLE_PROPERTY_SCROLLBAR_GUTTER, "stable both-edges");
  /* The leading gutter is not additional scroll height: content is 50px high.
   */
  dimensions(image, 100, 50, 100, 50);
  assert(divui_element_scroll_to(image, 1000.0, 1000.0) == DIVUI_OK);
  offset(image, 0.0, 0.0);
  property(image, DIVUI_STYLE_PROPERTY_SCROLLBAR_WIDTH, "none");
  dimensions(image, 100, 80, 100, 80);
  assert(divui_element_detach(image) == DIVUI_OK);
  dimensions(image, 0, 0, 0, 0);
  assert(divui_element_append_child(root, image) == DIVUI_OK);
  dimensions(image, 100, 80, 100, 80);
  assert(divui_element_destroy(image) == DIVUI_OK);
  assert(divui_resource_destroy(resource) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(document) == DIVUI_OK);
  printf("native C replaced scroll dimensions: scale=%g passed\n", scale);
}

static void body_consumer(double scale) {
  DivUiElement *root = NULL, *html = NULL, *body = NULL, *content = NULL;
  DivUiDocument* document = document_for_scale(scale, &root);
  assert(divui_element_create(document, DIVUI_ELEMENT_HTML, &html) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_BODY, &body) == DIVUI_OK);
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &content) == DIVUI_OK);
  assert(divui_element_append_child(root, html) == DIVUI_OK);
  assert(divui_element_append_child(html, body) == DIVUI_OK);
  assert(divui_element_append_child(body, content) == DIVUI_OK);
  authored_size(body);
  property(content, DIVUI_STYLE_PROPERTY_WIDTH, "150px");
  property(content, DIVUI_STYLE_PROPERTY_HEIGHT, "120px");
  property(body, DIVUI_STYLE_PROPERTY_OVERFLOW, "scroll");
  dimensions(root, 305, 225, 305, 225);
  dimensions(html, 305, 225, 305, 225);
  dimensions(body, 100, 80, 150, 120);
  assert(divui_element_scroll_to(body, 1000.0, 1000.0) == DIVUI_OK);
  offset(body, 0.0, 0.0);
  /* Disabling propagation transfers actual scroll ownership to the body. */
  property(html, DIVUI_STYLE_PROPERTY_OVERFLOW, "hidden");
  dimensions(body, 85, 65, 150, 120);
  assert(divui_element_scroll_to(body, 1000.0, 1000.0) == DIVUI_OK);
  offset(body, 65.0, 55.0);
  property(html, DIVUI_STYLE_PROPERTY_OVERFLOW, "visible");
  dimensions(body, 100, 80, 150, 120);
  offset(body, 0.0, 0.0);
  assert(divui_element_destroy(content) == DIVUI_OK);
  assert(divui_element_destroy(body) == DIVUI_OK);
  assert(divui_element_destroy(html) == DIVUI_OK);
  assert(divui_element_destroy(root) == DIVUI_OK);
  assert(divui_document_destroy(document) == DIVUI_OK);
  printf("native C body scroll dimensions: scale=%g passed\n", scale);
}

int main(void) {
  const double scales[] = {1.0, 1.25, 1.5, 1.75, 2.0};
  for (size_t i = 0; i < sizeof(scales) / sizeof(scales[0]); ++i) {
    consumer("flex", scales[i]);
    consumer("grid", scales[i]);
    table_consumer(scales[i]);
    replaced_consumer(scales[i]);
    body_consumer(scales[i]);
  }
  return 0;
}
