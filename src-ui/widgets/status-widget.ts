import { callRust } from "../lib/bindings.js";
import { Component, component } from "../lib/component.js";
import { img, p, Element } from "../lib/html.js";

export type StatusWidgetDef = {
  id: string;
  icon: string;
  label: string;
  tooltip?: string;
  actions?: StatusWidgetAction[];
};
export type StatusWidgetAction = {
  id: string;
  label: string;
  tooltip?: string;
};
export class StatusWidget extends Component {
  #icon: Element = img();
  #label: Element = p();

  set icon(value: string) {
    this.#icon.attr({ src: value ?? "" });
  }
  set label(value: string) {
    this.#label.clear();
    this.#label.text(value ?? "");
  }

  connectedCallback() {
    this.append(this.#icon, this.#label);
    this.on("click", this.onClick);
  }

  async onClick() {
    await callRust("status_widget_clicked", { id: this.id });
  }
}
export default component(StatusWidget);
