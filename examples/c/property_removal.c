/* Public C11/C++17 property ownership and callback removal guards. */
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "divui.h"

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

static DivUiElement* node(DivUiDocument* document, DivUiElement* parent) {
  DivUiElement* result = NULL;
  assert(divui_element_create(document, DIVUI_ELEMENT_DIV, &result) == DIVUI_OK);
  assert(divui_element_append_child(parent, result) == DIVUI_OK);
  return result;
}

static DivUiRect bounds(DivUiElement* element) {
  DivUiRect value;
  assert(divui_element_get_bounds(element, &value) == DIVUI_OK);
  return value;
}

static void remove_property(DivUiElement* element, DivUiStyleProperty id, uint8_t expected) {
  uint8_t removed = 99;
  assert(divui_element_remove_property(element, id, &removed) == DIVUI_OK);
  assert(removed == expected);
}

typedef struct CallbackState {
  DivUiElement* node;
  unsigned calls;
  DivUiStatus first, second, bounds_status;
  uint8_t removed[2];
  float width;
} CallbackState;

static void clicked(DivUiEvent* event, void* data) {
  CallbackState* state = (CallbackState*)data;
  DivUiRect value;
  (void)event;
  ++state->calls;
  /* No assertions, exceptions or unchecked reads in the C callback. */
  state->first =
      divui_element_remove_property(state->node, DIVUI_STYLE_PROPERTY_WIDTH, &state->removed[0]);
  state->second =
      divui_element_remove_property(state->node, DIVUI_STYLE_PROPERTY_WIDTH, &state->removed[1]);
  state->bounds_status = divui_element_get_bounds(state->node, &value);
  if (state->bounds_status == DIVUI_OK)
    state->width = value.width;
}

static void check_removal(double scale) {
  DivUiDocumentConfig config;
  memset(&config, 0, sizeof(config));
  config.struct_size = sizeof(config);
  config.abi_version = DIVUI_ABI_VERSION;
  config.viewport.logical_width = 320;
  config.viewport.logical_height = 240;
  config.viewport.physical_width = (uint32_t)(320 * scale);
  config.viewport.physical_height = (uint32_t)(240 * scale);
  config.viewport.device_scale_factor = scale;
  config.viewport.authority = DIVUI_VIEWPORT_LOGICAL;
  DivUiDocument* document = NULL;
  DivUiElement* root = NULL;
  assert(divui_document_create(&config, &document) == DIVUI_OK);
  assert(divui_document_root(document, &root) == DIVUI_OK);
  DivUiElement* owner = node(document, root);
  DivUiElement* child = node(document, owner);
  property(root, DIVUI_STYLE_PROPERTY_FONT_SIZE, "10px");
  property(owner, DIVUI_STYLE_PROPERTY_BOX_SIZING, "border-box");
  property(owner, DIVUI_STYLE_PROPERTY_WIDTH, "120px");
  property(owner, DIVUI_STYLE_PROPERTY_HEIGHT, "60px");
  property(child, DIVUI_STYLE_PROPERTY_WIDTH, "10px");
  property(child, DIVUI_STYLE_PROPERTY_HEIGHT, "5px");
  property(owner, DIVUI_STYLE_PROPERTY_PADDING, "1em");
  property(owner, DIVUI_STYLE_PROPERTY_PADDING_LEFT, "7px");
  assert(bounds(child).x - bounds(owner).x == 7);
  remove_property(owner, DIVUI_STYLE_PROPERTY_PADDING_LEFT, 1);
  assert(bounds(child).x - bounds(owner).x == 0);
  assert(bounds(child).y - bounds(owner).y == 10);
  property(root, DIVUI_STYLE_PROPERTY_FONT_SIZE, "20px");
  assert(bounds(child).x - bounds(owner).x == 0);
  assert(bounds(child).y - bounds(owner).y == 20);
  remove_property(owner, DIVUI_STYLE_PROPERTY_PADDING_LEFT, 0);
  remove_property(owner, DIVUI_STYLE_PROPERTY_PADDING, 1);
  assert(bounds(child).y - bounds(owner).y == 0);
  property(owner, DIVUI_STYLE_PROPERTY_DISPLAY, "none");
  remove_property(owner, DIVUI_STYLE_PROPERTY_DISPLAY, 1);
  assert(bounds(owner).width == 120);

  uint8_t unchanged = 99;
  assert(divui_element_remove_property(owner, DIVUI_STYLE_PROPERTY_WIDTH, NULL) ==
         DIVUI_ERROR_INVALID_ARGUMENT);
  assert(divui_element_remove_property(owner, (DivUiStyleProperty)INT32_MAX, &unchanged) ==
         DIVUI_ERROR_INVALID_ARGUMENT);
  assert(unchanged == 99 && bounds(owner).width == 120);
  assert(divui_element_remove_property((DivUiElement*)document, DIVUI_STYLE_PROPERTY_WIDTH,
                                       &unchanged) == DIVUI_ERROR_INVALID_HANDLE);
  assert(unchanged == 99 && bounds(owner).width == 120);

  /* The source reset stays masked by a real public width animation. */
  DivUiAnimationOptionsV1 options;
  memset(&options, 0, sizeof(options));
  options.struct_size = sizeof(options);
  options.abi_version = DIVUI_ABI_VERSION;
  options.duration_ms = 100;
  options.iterations = 1;
  options.playback_rate = 1;
  options.fill = 3;
  DivUiAnimationTimeline timeline;
  memset(&timeline, 0, sizeof(timeline));
  timeline.struct_size = sizeof(timeline);
  timeline.abi_version = DIVUI_ABI_VERSION;
  DivUiKeyframeV1 frames[2];
  memset(frames, 0, sizeof(frames));
  for (size_t i = 0; i < 2; ++i) {
    frames[i].struct_size = sizeof(frames[i]);
    frames[i].abi_version = DIVUI_ABI_VERSION;
    frames[i].offset = (double)i;
    frames[i].value.tag = DIVUI_STYLE_VALUE_LENGTH;
    frames[i].value.data.length.value = 40;
    frames[i].value.data.length.unit = DIVUI_LENGTH_PX;
  }
  uint64_t animation = 0;
  assert(divui_element_animate_v1(owner, DIVUI_STYLE_PROPERTY_WIDTH, frames, 2, sizeof(frames[0]),
                                  &options, &timeline, &animation) == DIVUI_OK);
  assert(animation != 0 && bounds(owner).width == 40);
  remove_property(owner, DIVUI_STYLE_PROPERTY_WIDTH, 1);
  assert(bounds(owner).width == 40);
  assert(divui_document_animation_cancel(document, animation) == DIVUI_OK);
  assert(bounds(owner).width == 320);
  remove_property(owner, DIVUI_STYLE_PROPERTY_WIDTH, 0);

  /* Raw em inheritance updates after source removal, without font idealization. */
  property(owner, DIVUI_STYLE_PROPERTY_FONT_SIZE, "30px");
  property(child, DIVUI_STYLE_PROPERTY_WIDTH, "2em");
  assert(bounds(child).width == 60);
  remove_property(owner, DIVUI_STYLE_PROPERTY_FONT_SIZE, 1);
  assert(bounds(child).width == 40);
  property(root, DIVUI_STYLE_PROPERTY_FONT_SIZE, "24px");
  assert(bounds(child).width == 48);

  property(owner, DIVUI_STYLE_PROPERTY_WIDTH, "77px");
  CallbackState state;
  memset(&state, 0, sizeof(state));
  state.node = owner;
  state.first = state.second = state.bounds_status = DIVUI_ERROR_INTERNAL;
  state.removed[0] = state.removed[1] = 99;
  DivUiListener* listener = NULL;
  assert(divui_element_add_event_listener(owner, DIVUI_EVENT_CLICK, 0, clicked, &state,
                                          &listener) == DIVUI_OK);
  DivUiEvent event;
  memset(&event, 0, sizeof(event));
  event.struct_size = sizeof(event);
  event.abi_version = DIVUI_ABI_VERSION;
  event.event_type = DIVUI_EVENT_CLICK;
  assert(divui_document_dispatch_event(document, owner, &event) == DIVUI_OK);
  assert(state.calls == 1 && state.first == DIVUI_OK && state.second == DIVUI_OK &&
         state.bounds_status == DIVUI_OK && state.removed[0] == 1 && state.removed[1] == 0 &&
         state.width == 320);
  assert(divui_listener_destroy(listener) == DIVUI_OK);
  assert(divui_element_destroy(child) == DIVUI_OK);
  assert(divui_element_destroy(owner) == DIVUI_OK);
  assert(divui_element_remove_property(owner, DIVUI_STYLE_PROPERTY_WIDTH, &unchanged) ==
         DIVUI_ERROR_INVALID_HANDLE);
  assert(unchanged == 99);
  assert(divui_document_destroy(document) == DIVUI_OK);
  assert(divui_element_remove_property(root, DIVUI_STYLE_PROPERTY_DISPLAY, &unchanged) ==
         DIVUI_ERROR_INVALID_HANDLE);
  assert(unchanged == 99);
  assert(divui_element_destroy(root) == DIVUI_OK);
  printf("native property removal: scale=%g passed\n", scale);
}

int main(void) {
  const double scales[] = {1.0, 1.25, 1.5, 1.75, 2.0};
  for (size_t i = 0; i < sizeof(scales) / sizeof(scales[0]); ++i)
    check_removal(scales[i]);
  return 0;
}
