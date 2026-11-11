#include "common.h"

int main(void) {
  DivUiColor color = {255, 152, 0, 255};
  return divui_example_render("Todo: ship Div v0.2", color) ? 0 : 1;
}
