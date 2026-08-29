import { Component, component } from "../lib/component.js";

class PatoHeaderCenter extends Component {
  connectedCallback() {
    this.setStyle({ flex: "1" });
  }
}
export default component(PatoHeaderCenter);
