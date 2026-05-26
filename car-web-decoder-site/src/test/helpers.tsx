import { render } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactElement } from "react";

export function renderUI(ui: ReactElement) {
  return render(ui);
}

export function setupUser() {
  return userEvent.setup();
}
