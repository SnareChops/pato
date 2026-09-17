import { bind } from "./bindings.js";
import type { Widget, WidgetSpec } from "pato:internal/widget-view@0.1.0";
import { PatoStatusWidgets } from "../components/pato-status-widgets.js";
import { PatoGrid } from "../components/pato-grid.js";

// Signature and payload shapes are checked against wit-internal/core-ui.wit
// (`widget-view`) via the jco-generated types in src-ui/generated/.

function grid(): PatoGrid | null {
  return document.querySelector<PatoGrid>("pato-grid");
}

bind(function widgetUpdate(widget: Widget): boolean {
  switch (widget.tag) {
    case "status-widget": {
      const statusWidgets = document.querySelector<PatoStatusWidgets>("pato-status-widgets");
      return statusWidgets ? statusWidgets.updateStatusWidget(widget.val) : false;
    }
    case "custom":
      return grid()?.updateCustom(widget.val) ?? false;
    default: {
      const _: never = widget;
      return false;
    }
  }
});

bind(function widgetRegister(spec: WidgetSpec): boolean {
  const g = grid();
  if (!g) return false;
  g.registerWidget(spec);
  return true;
});

bind(function widgetSetValue({ widgetId, slot, value }: { widgetId: string; slot: string; value: string }): boolean {
  grid()?.setValue(widgetId, slot, value);
  return true;
});

bind(function widgetRemove(widgetId: string): boolean {
  return grid()?.removeWidget(widgetId) ?? false;
});
