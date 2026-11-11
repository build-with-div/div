#include "divui.h"
static_assert(sizeof(DivUiEditCommandV1) == 32, "editing command size");
static_assert(alignof(DivUiEditCommandV1) == 4, "editing command alignment");
static_assert(offsetof(DivUiEditCommandV1, command) == 8, "editing command header");
static_assert(offsetof(DivUiEditCommandV1, reserved) == 24, "editing command reserved fields");
static_assert(sizeof(DivUiStyleValue) == 16, "frozen DivUiStyleValue size");
static_assert(sizeof(DivUiEvent) == 88, "frozen DivUiEvent size");
static_assert(sizeof(DivUiScrollMetricsV1) == 40, "scroll metrics size");
static_assert(alignof(DivUiScrollMetricsV1) == 8, "scroll metrics alignment");
static_assert(offsetof(DivUiScrollMetricsV1, client_width) == 8, "scroll metrics prefix");
static_assert(offsetof(DivUiScrollMetricsV1, scroll_height) == 32, "scroll metrics fields");
static_assert(sizeof(DivUiAccessibilitySnapshotInfo) == 56, "snapshot info size");
static_assert(sizeof(DivUiAccessibilityNodeInfo) == 120, "snapshot node size");
static_assert(sizeof(DivUiAccessibilityNodeState) == 112, "snapshot node state size");

int main() {
  return divui_abi_version() == DIVUI_ABI_VERSION ? 0 : 1;
}
