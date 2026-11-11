#include "common.h"

int main(void) {
  DivUiColor color = {103, 58, 183, 255};
  return divui_example_render("Div dashboard", color) ? 0 : 1;
}
