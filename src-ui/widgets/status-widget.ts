import { callRust } from "../lib/bindings.js";
import { Component, component } from "../lib/component.js";
import { button, div, img, p, Element } from "../lib/html.js";

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
  #menu: Element = div({ class: "status-widget-menu" });
  #actions: StatusWidgetAction[] = [];
  #open = false;

  set icon(value: string) {
    this.#icon.attr({ src: value ?? "" });
  }
  set label(value: string) {
    this.#label.clear();
    this.#label.text(value ?? "");
  }
  set tooltip(value: string | undefined) {
    this.title = value ?? "";
  }
  // Actions with none open (list is a fresh reference every widget update,
  // even when unchanged - rebuild unconditionally, it's a handful of nodes).
  set actions(value: StatusWidgetAction[] | undefined) {
    this.#actions = value ?? [];
    this.#setOpen(false);
    this.#renderMenu();
    // Only a widget with something to do invites a click.
    this.classList.toggle("has-actions", this.#actions.length > 0);
  }

  connectedCallback() {
    this.setStyle({ position: "relative" });
    this.append(this.#icon, this.#label, this.#menu);
    this.on("click", (ev: Event) => this.onClick(ev));
    // Closing the dropdown on an outside click is the standard pattern for
    // this kind of menu; `capture` so it still fires even if the click
    // target is later removed by the same re-render this triggers.
    document.addEventListener("click", this.#onDocumentClick, { capture: true });
  }

  disconnectedCallback() {
    document.removeEventListener("click", this.#onDocumentClick, { capture: true });
  }

  #onDocumentClick = (ev: MouseEvent) => {
    if (this.#open && !this.contains(ev.target as Node)) this.#setOpen(false);
  };

  onClick(ev: Event) {
    // Every interaction resolves to a named action - a widget with nothing
    // to do declares no actions and a click is a no-op, full stop.
    if (this.#actions.length === 0) return;
    ev.stopPropagation();
    this.#setOpen(!this.#open);
  }

  #setOpen(open: boolean) {
    this.#open = open;
    this.#menu.class(open ? "status-widget-menu open" : "status-widget-menu");
  }

  #renderMenu() {
    this.#menu.clear();
    for (const action of this.#actions) {
      const item = button({ title: action.tooltip ?? "" }, action.label);
      item.on("click", async (ev: Event) => {
        ev.stopPropagation();
        this.#setOpen(false);
        await callRust("status_widget_action", { id: this.id, action: action.id });
      });
      this.#menu.append(item);
    }
  }
}
export default component(StatusWidget);
