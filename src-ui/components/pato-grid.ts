import { callRust } from "../lib/bindings.js";
import { Component, component } from "../lib/component.js";
import { PatoCustomWidget } from "../widgets/custom-widget.js";
import type { CustomWidget, WidgetSpec } from "pato:internal/widget-view@0.1.0";
import type { TileSize } from "pato:internal/widget-dom@0.1.0";

/** Column count — keep in sync with `grid-template-columns` in index.css. */
export const GRID_COLS = 30;

type Cell = {
  el: HTMLDivElement;
  widget: PatoCustomWidget;
  size: TileSize;
  min: TileSize;
  max: TileSize;
};

function clampTiles(t: TileSize, min: TileSize, max: TileSize): TileSize {
  return {
    w: Math.max(min.w, Math.min(max.w, t.w)),
    h: Math.max(min.h, Math.min(max.h, t.h)),
  };
}

function loadSize(id: string): TileSize | undefined {
  try {
    const raw = localStorage.getItem(`pato:widget-size:${id}`);
    if (raw) return JSON.parse(raw) as TileSize;
  } catch {}
  return undefined;
}
function saveSize(id: string, size: TileSize) {
  try {
    localStorage.setItem(`pato:widget-size:${id}`, JSON.stringify(size));
  } catch {}
}

export class PatoGrid extends Component {
  #cells = new Map<string, Cell>();
  #tileObserver?: ResizeObserver;

  connectedCallback() {
    this.#tileObserver ??= new ResizeObserver(() => this.#syncTile());
    this.#tileObserver.observe(this);
    this.#syncTile();
  }

  disconnectedCallback() {
    this.#tileObserver?.disconnect();
  }

  /**
   * Publish the real pixel width of one `1fr` column as `--pato-grid-tile` so the
   * CSS gap-line background (and `grid-auto-rows`) line up with the actual tracks.
   */
  #syncTile() {
    const styles = getComputedStyle(this);
    const gap = parseFloat(styles.columnGap) || 0;
    const scrollbar = this.offsetWidth - this.clientWidth; // vertical scrollbar, if any
    const inner =
      this.getBoundingClientRect().width -
      scrollbar -
      (parseFloat(styles.paddingLeft) || 0) -
      (parseFloat(styles.paddingRight) || 0);
    if (inner <= 0) return;
    const tile = (inner - gap * (GRID_COLS - 1)) / GRID_COLS;
    this.style.setProperty("--pato-grid-tile", `${tile}px`);
  }

  /** Reserve a grid cell for a widget and report its layout to the core. */
  registerWidget(spec: WidgetSpec) {
    const min = spec.min ?? spec.size;
    const max = spec.max ?? spec.size;
    const cell = this.#ensureCell(spec.id);
    cell.min = min;
    cell.max = max;
    const size = clampTiles(loadSize(spec.id) ?? spec.size, min, max);
    this.#resize(spec.id, size, false);
    this.#setHandle(cell);
    this.#reportLayout(spec.id, size);
  }

  /** Render (or re-render) a custom widget's tree. */
  updateCustom(widget: CustomWidget): boolean {
    const cell = this.#ensureCell(widget.id);
    cell.widget.render(widget.nodes);
    return true;
  }

  setValue(widgetId: string, slot: string, value: string) {
    this.#cells.get(widgetId)?.widget.setValue(slot, value);
  }

  removeWidget(widgetId: string): boolean {
    const cell = this.#cells.get(widgetId);
    if (!cell) return false;
    cell.el.remove();
    this.#cells.delete(widgetId);
    return true;
  }

  #ensureCell(id: string): Cell {
    let cell = this.#cells.get(id);
    if (cell) return cell;

    const widget = new PatoCustomWidget();
    widget.widgetId = id;
    const el = document.createElement("div");
    el.className = "pato-cell";
    el.dataset.widgetId = id;
    el.append(widget);
    this.append(el);

    cell = { el, widget, size: { w: 4, h: 4 }, min: { w: 1, h: 1 }, max: { w: GRID_COLS, h: GRID_COLS } };
    this.#cells.set(id, cell);
    this.#resize(id, cell.size, false);
    return cell;
  }

  #resize(id: string, size: TileSize, persist: boolean) {
    const cell = this.#cells.get(id);
    if (!cell) return;
    cell.size = size;
    cell.el.style.gridColumn = `span ${size.w}`;
    cell.el.style.gridRow = `span ${size.h}`;
    cell.el.style.aspectRatio = `${size.w} / ${size.h}`;
    if (persist) saveSize(id, size);
  }

  #reportLayout(id: string, size: TileSize) {
    void callRust("widget_resized", { widgetId: id, w: size.w, h: size.h });
  }

  /** Add a corner drag handle when the widget is user-resizable. */
  #setHandle(cell: Cell) {
    const resizable = cell.min.w !== cell.max.w || cell.min.h !== cell.max.h;
    const existing = cell.el.querySelector<HTMLDivElement>(".pato-resize");
    if (!resizable) {
      existing?.remove();
      return;
    }
    if (existing) return;

    const handle = document.createElement("div");
    handle.className = "pato-resize";
    handle.addEventListener("pointerdown", (down) => {
      down.preventDefault();
      handle.setPointerCapture(down.pointerId);
      const rect = this.getBoundingClientRect();
      const tile = rect.width / GRID_COLS;
      const origin = cell.el.getBoundingClientRect();

      const move = (m: PointerEvent) => {
        const next = clampTiles(
          {
            w: Math.max(1, Math.round((m.clientX - origin.left) / tile)),
            h: Math.max(1, Math.round((m.clientY - origin.top) / tile)),
          },
          cell.min,
          cell.max,
        );
        if (next.w !== cell.size.w || next.h !== cell.size.h) this.#resize(cell.el.dataset.widgetId!, next, false);
      };
      const up = () => {
        handle.releasePointerCapture(down.pointerId);
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", up);
        const id = cell.el.dataset.widgetId!;
        this.#resize(id, cell.size, true);
        this.#reportLayout(id, cell.size);
      };
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
    });
    cell.el.append(handle);
  }
}
export default component(PatoGrid);
