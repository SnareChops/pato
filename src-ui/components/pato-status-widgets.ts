import { component, Component } from "../lib/component.js";
import { StatusWidget } from "../widgets/status-widget.js";
import type { StatusWidget as StatusWidgetView } from "pato:internal/widget-view@0.1.0";

export class PatoStatusWidgets extends Component {
  #widgets: StatusWidget[] = [];

  add(widget: StatusWidget) {
    this.#widgets.push(widget);
    this.append(widget);
  }

  updateStatusWidget(props: StatusWidgetView) {
    if (!props.id) return false;
    console.log("Updating status widget", props);
    let widget: StatusWidget | null = this.querySelector<StatusWidget>(`status-widget[id="${props.id}"]`);
    if (!widget) {
      widget = new StatusWidget({ id: props.id });
      this.#widgets.push(widget);
      this.append(widget);
    }
    for (const key in props) {
      //@ts-ignore
      console.log("setting", key, "to", props[key]);
      // @ts-ignore
      widget[key] = props[key];
    }
    return true;
  }
}
export default component(PatoStatusWidgets);
