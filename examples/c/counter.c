#include "common.h"

int main(void) {
  DivUiColor color = {76, 175, 80, 255};
  return divui_example_render("Counter: 0", color) ? 0 : 1;
}
