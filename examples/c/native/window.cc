#include <cstdio>
#include <cstdlib>
#include <cstring>

#include "divui.h"

static_assert(sizeof(DivUiAppConfig) == 40, "existing app configuration");
static_assert(sizeof(DivUiEvent) == 88, "existing event layout");
static_assert(sizeof(DivUiAppRunConfig) == 32, "versioned run configuration");
static_assert(sizeof(DivUiPlatformEvent) == 152, "versioned platform event");

struct Context {
  DivUiDocument* document = nullptr;
  DivUiElement* card = nullptr;
  DivUiElement* textarea = nullptr;
  DivUiListener* input_listener = nullptr;
  DivUiListener* focus_listener = nullptr;
  DivUiListener* blur_listener = nullptr;
  unsigned input_events = 0;
  unsigned focus_events = 0;
  unsigned blur_events = 0;
  unsigned backend = DIVUI_BACKEND_SOFTWARE;
  uint64_t frames = 0;
  bool failed = false;
};

static bool check(Context& context, DivUiStatus status) {
  if (status == DIVUI_OK)
    return true;
  context.failed = true;
  std::fprintf(stderr, "native C++ status: %d\n", status);
  return false;
}

extern "C" void on_input(DivUiEvent* event, void* user_data) {
  auto& context = *static_cast<Context*>(user_data);
  const char* values[] = {"á👩‍💻", "á👩‍💻z", "á👩‍💻"};
  ++context.input_events;
  if (context.input_events > 3) {
    context.failed = true;
    return;
  }
  const char* expected = values[context.input_events - 1];
  uint8_t bytes[64];
  size_t length = 0;
  if (event->event_type != DIVUI_EVENT_INPUT || event->target != context.textarea ||
      event->current_target != context.textarea ||
      !check(context,
             divui_element_copy_control_value(context.textarea, bytes, sizeof(bytes), &length)) ||
      length != std::strlen(expected) || std::memcmp(bytes, expected, length) != 0)
    context.failed = true;
  const char label[] = "C++ input callback";
  check(context, divui_element_set_text(
                     context.card, {reinterpret_cast<const uint8_t*>(label), std::strlen(label)}));
}

extern "C" void on_focus(DivUiEvent* event, void* user_data) {
  auto& context = *static_cast<Context*>(user_data);
  if (event->target != context.textarea || event->current_target != context.textarea)
    context.failed = true;
  if (event->event_type == DIVUI_EVENT_FOCUS)
    ++context.focus_events;
  else if (event->event_type == DIVUI_EVENT_BLUR)
    ++context.blur_events;
  else
    context.failed = true;
  const char key[] = "data-focus";
  const char value[] = "seen";
  check(context, divui_element_set_attribute(
                     context.card, {reinterpret_cast<const uint8_t*>(key), std::strlen(key)},
                     {reinterpret_cast<const uint8_t*>(value), std::strlen(value)}));
}

extern "C" void on_platform(DivUiApp* app, const DivUiPlatformEvent* event, void* user_data) {
  auto& context = *static_cast<Context*>(user_data);
  if (event->event_type == DIVUI_PLATFORM_BACKEND_CHANGED && event->backend != context.backend)
    context.failed = true;
  if (event->event_type == DIVUI_PLATFORM_PRESENTED) {
    context.frames = event->frame_number;
    if (context.frames == 1) {
      check(context, divui_element_focus(context.textarea));
      check(context, divui_element_focus(context.textarea));
      check(context, divui_element_blur(context.textarea));
      check(context, divui_element_blur(context.textarea));
      check(context, divui_element_focus(context.textarea));
      if (context.focus_events != 2 || context.blur_events != 1)
        context.failed = true;
      const char tabindex[] = "tabindex";
      const char negative[] = "-1";
      check(context,
            divui_element_set_attribute(
                context.card, {reinterpret_cast<const uint8_t*>(tabindex), std::strlen(tabindex)},
                {reinterpret_cast<const uint8_t*>(negative), std::strlen(negative)}));
      check(context, divui_element_focus(context.card));
      DivUiElement* focused = nullptr;
      check(context, divui_document_advance_focus(context.document, 1, &focused));
      if (focused != context.textarea)
        context.failed = true;
      check(context, divui_document_set_modal_root(context.document, context.card));
      check(context, divui_document_set_modal_root(context.document, nullptr));
      if (context.focus_events != 4 || context.blur_events != 3)
        context.failed = true;
      DivUiEditCommandV1 edit{sizeof(edit),
                              DIVUI_ABI_VERSION,
                              DIVUI_EDIT_DELETE,
                              DIVUI_TEXT_BACKWARD,
                              DIVUI_TEXT_GRAPHEME,
                              0,
                              {0, 0}};
      check(context, divui_element_edit_text_v1(context.textarea, &edit));
      edit.command = DIVUI_EDIT_UNDO;
      check(context, divui_element_edit_text_v1(context.textarea, &edit));
      edit.command = DIVUI_EDIT_REDO;
      check(context, divui_element_edit_text_v1(context.textarea, &edit));
      edit.command = DIVUI_EDIT_SELECT_ALL;
      check(context, divui_element_edit_text_v1(context.textarea, &edit));
      if (context.input_events != 3)
        context.failed = true;
    } else {
      check(context, divui_app_request_exit(app));
    }
  }
  if (context.failed)
    divui_app_request_exit(app);
}

int main(int argc, char** argv) {
  Context context;
  if (argc > 1)
    context.backend = static_cast<unsigned>(std::atoi(argv[1]));
  const char title[] = "Div native C++";
  DivUiAppConfig config{sizeof(config),
                        DIVUI_ABI_VERSION,
                        {reinterpret_cast<const uint8_t*>(title), std::strlen(title)},
                        240,
                        120,
                        context.backend,
                        0};
  DivUiApp* app = nullptr;
  DivUiElement* root = nullptr;
  if (!check(context, divui_app_create(&config, &app)) ||
      !check(context, divui_app_document(app, &context.document)) ||
      !check(context, divui_document_root(context.document, &root)) ||
      !check(context, divui_element_create(context.document, DIVUI_ELEMENT_DIV, &context.card)) ||
      !check(context, divui_element_append_child(root, context.card)))
    return 1;
  const char value[] = "á👩‍💻z";
  check(context, divui_element_create(context.document, DIVUI_ELEMENT_TEXTAREA, &context.textarea));
  check(context, divui_element_append_child(root, context.textarea));
  check(context,
        divui_element_set_control_value(
            context.textarea, {reinterpret_cast<const uint8_t*>(value), std::strlen(value)}));
  check(context,
        divui_element_set_selection(context.textarea, std::strlen(value), std::strlen(value)));
  check(context, divui_element_add_event_listener(context.textarea, DIVUI_EVENT_INPUT, 0, on_input,
                                                  &context, &context.input_listener));
  check(context, divui_element_add_event_listener(context.textarea, DIVUI_EVENT_FOCUS, 0, on_focus,
                                                  &context, &context.focus_listener));
  check(context, divui_element_add_event_listener(context.textarea, DIVUI_EVENT_BLUR, 0, on_focus,
                                                  &context, &context.blur_listener));
  DivUiAppRunConfig run{sizeof(run), DIVUI_ABI_VERSION, 0, 0, on_platform, &context};
  check(context, divui_app_run(app, &run));
  check(context, divui_listener_destroy(context.input_listener));
  check(context, divui_listener_destroy(context.focus_listener));
  check(context, divui_listener_destroy(context.blur_listener));
  check(context, divui_element_destroy(context.textarea));
  check(context, divui_element_destroy(context.card));
  check(context, divui_element_destroy(root));
  check(context, divui_document_destroy(context.document));
  check(context, divui_app_destroy(app));
  std::printf("native C++: backend=%u frames=%llu input=%u focus=%u blur=%u\n", context.backend,
              static_cast<unsigned long long>(context.frames), context.input_events,
              context.focus_events, context.blur_events);
  return context.failed || context.frames < 2 || context.input_events != 3 ||
         context.focus_events != 4 || context.blur_events != 3;
}
