#include "common.h"

int main(void) {
  DivUiColor color = {33, 150, 243, 255};
  return divui_example_render("Hello from Div", color) ? 0 : 1;
}
