import { bind } from "./bindings.js";
import type { Widget } from "pato:internal/widget-view@0.1.0";
import { PatoStatusWidgets } from "../components/pato-status-widgets.js";

// Signature and payload shape are checked against wit-internal/core-ui.wit
// (`widget-view.update`) via the jco-generated types in src-ui/generated/.
// When `widget` variant gains a second case, add a
// `default: { const _: never = widget; return false; }` for exhaustiveness.
bind(function widgetUpdate(widget: Widget): boolean {
  console.log("Updating widget from Rust:", widget);
  switch (widget.tag) {
    case "status-widget": {
      const statusWidgets = document.querySelector<PatoStatusWidgets>("pato-status-widgets");
      return statusWidgets ? statusWidgets.updateStatusWidget(widget.val) : false;
    }
  }
});
