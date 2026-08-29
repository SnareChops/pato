import { Component, component } from "../lib/component.js";
import { span, h1 } from "../lib/html.js";

class PatoHeaderLeft extends Component {
  connectedCallback() {
    this.setStyle({
      display: "flex",
      alignItems: "center",
      gap: "12px",
    });
    this.append(
      span({ style: 'fontSize: "28px"; line-height: 1;' }, "🦆"),
      h1(
        {
          style:
            'font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; font-size: 20px; font-weight: 600; color: white; margin: 0; text-shadow: 0 1px 2px rgba(0,0,0,0.2);',
        },
        "Pato"
      )
    );
  }
}
export default component(PatoHeaderLeft);
