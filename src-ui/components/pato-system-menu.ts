import { Component, component } from "../lib/component.js";
import { onLockChange, toggleLocked } from "../lib/dashboard-lock.js";
import { button, div, Element } from "../lib/html.js";

// Core-only settings menu for Pato itself, distinct from any plugin's
// status widget dropdown. Items land here as the system-level options they
// represent get built; dashboard lock is the first.
export class PatoSystemMenu extends Component {
  #button: Element = button({ class: "system-menu-button", title: "Pato settings" }, "⚙");
  #menu: Element = div({ class: "system-menu" });
  #lockItem: Element = button({}, "");
  #open = false;
  #unsubscribeLock?: () => void;

  connectedCallback() {
    this.setStyle({ position: "relative" });
    this.#lockItem.on("click", (ev: Event) => this.#onLockClick(ev));
    this.#menu.append(this.#lockItem);
    this.append(this.#button, this.#menu);
    this.#button.on("click", (ev: Event) => this.#onClick(ev));
    // capture so it still fires even if the click target is later removed
    // by the same re-render this triggers, matching status-widget's pattern.
    document.addEventListener("click", this.#onDocumentClick, { capture: true });
    this.#unsubscribeLock ??= onLockChange((locked) => this.#renderLockItem(locked));
  }

  disconnectedCallback() {
    document.removeEventListener("click", this.#onDocumentClick, { capture: true });
    this.#unsubscribeLock?.();
  }

  #renderLockItem(locked: boolean) {
    this.#lockItem.clear();
    this.#lockItem.text(locked ? "Unlock Dashboard" : "Lock Dashboard");
  }

  #onLockClick(ev: Event) {
    ev.stopPropagation();
    toggleLocked();
    this.#setOpen(false);
  }

  #onDocumentClick = (ev: MouseEvent) => {
    if (this.#open && !this.contains(ev.target as Node)) this.#setOpen(false);
  };

  #onClick(ev: Event) {
    ev.stopPropagation();
    this.#setOpen(!this.#open);
  }

  #setOpen(open: boolean) {
    this.#open = open;
    this.#menu.class(open ? "system-menu open" : "system-menu");
  }
}
export default component(PatoSystemMenu);
