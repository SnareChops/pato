import { Component, component } from "../lib/component.js";

class PatoHeader extends Component {
  connectedCallback() {
    this.setStyle({
      display: "flex",
      width: "100%",
      background: "linear-gradient(135deg, #1e3a8a 0%, #1d4ed8 100%)",
      boxShadow: "0 2px 8px rgba(0, 0, 0, 0.15)",
      borderBottom: "1px solid rgba(255, 255, 255, 0.1)",
      alignItems: "center",
      justifyContent: "space-between",
      padding: "12px 20px",
      height: "60px",
      boxSizing: "border-box",
    });
  }
}
export default component(PatoHeader);
