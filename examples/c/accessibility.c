#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "divui.h"

#define CHECK(call)                                           \
  do {                                                        \
    DivUiStatus status = (call);                              \
    if (status != DIVUI_OK) {                                 \
      fprintf(stderr, "%s failed: %d\n", #call, (int)status); \
      failed = 1;                                             \
      goto cleanup;                                           \
    }                                                         \
  } while (0)

static DivUiUtf8 text(const char* value) {
  DivUiUtf8 result = {(const uint8_t*)value, strlen(value)};
  return result;
}

int main(void) {
  DivUiDocumentConfig config = {
      sizeof(config), DIVUI_ABI_VERSION, {100.0, 100.0, 100, 100, 1.0, DIVUI_VIEWPORT_LOGICAL, 0}};
  DivUiDocument* document = NULL;
  DivUiElement* root = NULL;
  DivUiElement* button = NULL;
  DivUiElement* text_control = NULL;
  DivUiAccessibilitySnapshot* first = NULL;
  DivUiAccessibilitySnapshot* second = NULL;
  DivUiAccessibilitySnapshot* third = NULL;
  DivUiAccessibilitySnapshot* read_only = NULL;
  int failed = 0;
  uint64_t root_id = 0;
  uint64_t button_id = 0;
  CHECK(divui_document_create(&config, &document));
  CHECK(divui_document_root(document, &root));
  CHECK(divui_element_create(document, DIVUI_ELEMENT_BUTTON, &button));
  CHECK(divui_element_append_child(root, button));
  CHECK(divui_element_set_accessibility_label(button, text("Run")));
  CHECK(divui_element_set_accessibility_live(button, DIVUI_ACCESSIBILITY_LIVE_POLITE));
  CHECK(divui_element_get_accessibility_id(root, &root_id));
  CHECK(divui_element_get_accessibility_id(button, &button_id));
  CHECK(divui_document_accessibility_snapshot(document, NULL, &first));

  DivUiAccessibilitySnapshotInfo snapshot_info = {.struct_size = sizeof(snapshot_info),
                                                  .abi_version = DIVUI_ABI_VERSION};
  CHECK(divui_accessibility_snapshot_get_info(first, &snapshot_info));
  if (!snapshot_info.full_tree || snapshot_info.node_count < 2 ||
      snapshot_info.changed_count != snapshot_info.node_count) {
    failed = 1;
    goto cleanup;
  }
  int found_button = 0;
  for (size_t index = 0; index < snapshot_info.node_count; ++index) {
    DivUiAccessibilityNodeInfo node = {.struct_size = sizeof(node),
                                       .abi_version = DIVUI_ABI_VERSION};
    CHECK(divui_accessibility_snapshot_get_node(first, index, &node));
    if (node.id == button_id) {
      found_button = node.role == DIVUI_ACCESSIBILITY_ROLE_BUTTON && node.label_length == 3 &&
                     (node.actions & DIVUI_ACCESSIBILITY_ACTION_CLICK_BIT) != 0;
    }
  }
  if (!found_button) {
    failed = 1;
    goto cleanup;
  }
  DivUiAccessibilityNodeState state = {.struct_size = sizeof(state),
                                       .abi_version = DIVUI_ABI_VERSION};
  CHECK(divui_accessibility_snapshot_get_node_state(first, button_id, &state));
  if (state.live != DIVUI_ACCESSIBILITY_LIVE_POLITE + 1) {
    failed = 1;
    goto cleanup;
  }
  uint64_t child_id = 0;
  size_t child_count = 0;
  CHECK(divui_accessibility_snapshot_copy_ids(first, root_id, DIVUI_ACCESSIBILITY_IDS_CHILDREN,
                                              &child_id, 1, &child_count));
  if (child_count != 1 || child_id != button_id) {
    failed = 1;
    goto cleanup;
  }
  uint8_t label[3];
  size_t length = 0;
  CHECK(divui_accessibility_snapshot_copy_text(first, button_id, DIVUI_ACCESSIBILITY_TEXT_LABEL,
                                               NULL, 0, &length));
  if (length != 3 ||
      divui_accessibility_snapshot_copy_text(first, button_id, DIVUI_ACCESSIBILITY_TEXT_LABEL,
                                             label, 2, &length) != DIVUI_ERROR_BUFFER_TOO_SMALL) {
    failed = 1;
    goto cleanup;
  }
  CHECK(divui_accessibility_snapshot_copy_text(first, button_id, DIVUI_ACCESSIBILITY_TEXT_LABEL,
                                               label, sizeof(label), &length));
  if (length != 3 || memcmp(label, "Run", 3) != 0) {
    failed = 1;
    goto cleanup;
  }

  CHECK(divui_element_set_accessibility_label(button, text("Go")));
  CHECK(divui_document_accessibility_snapshot(document, first, &second));
  CHECK(divui_accessibility_snapshot_get_info(second, &snapshot_info));
  if (snapshot_info.full_tree || snapshot_info.changed_count != 1 ||
      snapshot_info.removed_count != 0) {
    failed = 1;
    goto cleanup;
  }
  uint64_t changed_id = 0;
  size_t changed_count = 0;
  CHECK(divui_accessibility_snapshot_copy_changes(second, 0, &changed_id, 1, &changed_count));
  if (changed_count != 1 || changed_id != button_id) {
    failed = 1;
    goto cleanup;
  }

  CHECK(divui_element_remove(button));
  CHECK(divui_document_accessibility_snapshot(document, second, &third));
  CHECK(divui_accessibility_snapshot_get_info(third, &snapshot_info));
  if (snapshot_info.removed_count != 1) {
    failed = 1;
    goto cleanup;
  }
  uint64_t removed_id = 0;
  size_t removed_count = 0;
  CHECK(divui_accessibility_snapshot_copy_changes(third, 1, &removed_id, 1, &removed_count));
  if (removed_count != 1 || removed_id != button_id) {
    failed = 1;
    goto cleanup;
  }

  const DivUiElementTag text_kinds[] = {DIVUI_ELEMENT_INPUT, DIVUI_ELEMENT_TEXTAREA};
  for (size_t kind = 0; kind < sizeof(text_kinds) / sizeof(text_kinds[0]); ++kind) {
    CHECK(divui_element_create(document, text_kinds[kind], &text_control));
    CHECK(divui_element_append_child(root, text_control));
    CHECK(divui_element_set_control_value(text_control, text("original")));
    CHECK(divui_element_set_attribute(text_control, text("readonly"), text("")));
    CHECK(divui_element_focus(text_control));
    DivUiEvent key = {.struct_size = sizeof(key),
                      .abi_version = DIVUI_ABI_VERSION,
                      .event_type = DIVUI_EVENT_KEY_DOWN,
                      .key_code = 13,
                      .text = text("Enter")};
    CHECK(divui_document_dispatch_key_input_v1(document, &key, text("\r")));
    CHECK(divui_document_dispatch_text_input_v1(document, text("blocked")));
    if (divui_element_perform_accessibility_action(text_control, DIVUI_ACCESSIBILITY_SET_VALUE,
                                                   text("blocked"), 0, 0) == DIVUI_OK ||
        divui_element_perform_accessibility_action(text_control,
                                                   DIVUI_ACCESSIBILITY_REPLACE_SELECTED_TEXT,
                                                   text("blocked"), 0, 0) == DIVUI_OK) {
      failed = 1;
      goto cleanup;
    }
    CHECK(divui_element_perform_accessibility_action(
        text_control, DIVUI_ACCESSIBILITY_SET_TEXT_SELECTION, text(""), 0, 8));
    uint8_t value[8];
    CHECK(divui_element_copy_control_value(text_control, value, sizeof(value), &length));
    if (length != sizeof(value) || memcmp(value, "original", sizeof(value)) != 0) {
      failed = 1;
      goto cleanup;
    }
    uint64_t control_id = 0;
    CHECK(divui_element_get_accessibility_id(text_control, &control_id));
    CHECK(divui_document_accessibility_snapshot(document, NULL, &read_only));
    CHECK(divui_accessibility_snapshot_get_info(read_only, &snapshot_info));
    int found_control = 0;
    for (size_t index = 0; index < snapshot_info.node_count; ++index) {
      DivUiAccessibilityNodeInfo node = {.struct_size = sizeof(node),
                                         .abi_version = DIVUI_ABI_VERSION};
      CHECK(divui_accessibility_snapshot_get_node(read_only, index, &node));
      if (node.id == control_id) {
        found_control =
            (node.flags & DIVUI_ACCESSIBILITY_NODE_READ_ONLY) != 0 &&
            (node.actions & DIVUI_ACCESSIBILITY_ACTION_SET_TEXT_SELECTION_BIT) != 0 &&
            (node.actions & (DIVUI_ACCESSIBILITY_ACTION_SET_VALUE_BIT |
                             DIVUI_ACCESSIBILITY_ACTION_REPLACE_SELECTED_TEXT_BIT)) == 0;
      }
    }
    if (!found_control) {
      failed = 1;
      goto cleanup;
    }
    CHECK(divui_element_set_control_value(text_control, text("native")));
    CHECK(divui_accessibility_snapshot_destroy(read_only));
    read_only = NULL;
    CHECK(divui_element_remove(text_control));
    CHECK(divui_element_destroy(text_control));
    text_control = NULL;
  }
  CHECK(divui_document_destroy(document));
  document = NULL;
  CHECK(divui_accessibility_snapshot_copy_text(first, button_id, DIVUI_ACCESSIBILITY_TEXT_LABEL,
                                               label, sizeof(label), &length));
  if (length != 3 || memcmp(label, "Run", 3) != 0) {
    failed = 1;
    goto cleanup;
  }
  puts("Owned accessibility snapshots and incremental changes: OK");

cleanup:
  if (read_only)
    divui_accessibility_snapshot_destroy(read_only);
  if (text_control)
    divui_element_destroy(text_control);
  if (third)
    divui_accessibility_snapshot_destroy(third);
  if (second)
    divui_accessibility_snapshot_destroy(second);
  if (first)
    divui_accessibility_snapshot_destroy(first);
  if (button)
    divui_element_destroy(button);
  if (root)
    divui_element_destroy(root);
  if (document)
    divui_document_destroy(document);
  return failed;
}
