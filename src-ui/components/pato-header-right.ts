import { Component, component } from "../lib/component.js";

class PatoHeaderRight extends Component {
  connectedCallback() {
    this.setStyle({
      display: "flex",
      alignItems: "center",
      gap: "8px",
    });
  }
}
export default component(PatoHeaderRight);
