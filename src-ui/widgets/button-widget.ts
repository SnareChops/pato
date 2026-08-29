import { Component, component } from "../lib/component.js";
import { img, p } from "../lib/html.js";

class ButtonWidget extends Component {
  #label;
  #icon;

  constructor() {
    super();
    this.on("click", this.onClick);
    this.#label = p({ class: "button-label" }).el();
    this.#icon = img({ class: "button-icon" }).el();
    this.append(this.#icon, this.#label);
  }

  connectedCallback() {}

  color(value: string): this {
    this.setStyle({ backgroundColor: value ?? "transparent" });
    return this;
  }

  label(value: string): this {
    this.#label.textContent = value;
    return this;
  }

  icon(value: string): this {
    this.#icon.setAttribute("src", value ?? "");
    return this;
  }

  pos(x: number, y: number, w: number, h: number): this {
    this.style.gridArea = `${y} / ${x} / span ${h} / span ${w}`;
    return this;
  }

  onClick() {}
}
export default component(ButtonWidget);
