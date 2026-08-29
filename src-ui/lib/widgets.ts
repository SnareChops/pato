import { bind } from "./bindings.js";
import { PatoStatusWidgets } from "../components/pato-status-widgets.js";

type ActionDef = {
  id: string;
  label: string;
  tooltip?: string;
};
type WidgetDef = {
  type: "status";
  id: string;
  icon: string;
  label: string;
  tooltip?: string;
  actions?: ActionDef[];
};
bind(function widgetUpdate(widget: WidgetDef): void {
  console.log("Updating widget from Rust:", widget);
  switch (widget.type) {
    case "status":
      const statusWidgets = document.querySelector<PatoStatusWidgets>("pato-status-widgets");
      if (statusWidgets) statusWidgets.updateStatusWidget(widget);
  }
});
