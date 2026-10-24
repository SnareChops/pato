import { callRust } from "../lib/bindings.js";
import { Component, component } from "../lib/component.js";
import { onLockChange } from "../lib/dashboard-lock.js";
import { PatoCustomWidget } from "../widgets/custom-widget.js";
import type { CustomWidget, WidgetSpec } from "pato:internal/widget-view@0.1.0";
import type { TileSize } from "pato:internal/widget-dom@0.1.0";

/** Column count — keep in sync with `grid-template-columns` in index.css. */
export const GRID_COLS = 30;

type GridPos = { x: number; y: number };

type Cell = {
  el: HTMLDivElement;
  widget: PatoCustomWidget;
  pos: GridPos;
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

// Saved layout lives in localStorage keyed by widget id, and outlives the
// widgets themselves: plugins get updated, and widgets get added, removed or
// resized between runs. So nothing read back can be trusted - entries for
// widgets that no longer exist are simply never read (and are kept, so a
// widget that returns finds its old spot), and entries for ones that do
// exist are shape-checked here and collision-checked where they are applied.
function readJson(key: string): unknown {
  try {
    const raw = localStorage.getItem(key);
    return raw ? JSON.parse(raw) : undefined;
  } catch {
    return undefined;
  }
}
function isCount(n: unknown): n is number {
  return typeof n === "number" && Number.isInteger(n) && n >= 1 && n <= GRID_COLS * 100;
}
function loadSize(id: string): TileSize | undefined {
  const v = readJson(`pato:widget-size:${id}`) as Partial<TileSize> | null | undefined;
  return v && isCount(v.w) && isCount(v.h) ? { w: v.w, h: v.h } : undefined;
}
function saveSize(id: string, size: TileSize) {
  try {
    localStorage.setItem(`pato:widget-size:${id}`, JSON.stringify(size));
  } catch {}
}

function loadPos(id: string): GridPos | undefined {
  const v = readJson(`pato:widget-pos:${id}`) as Partial<GridPos> | null | undefined;
  return v && isCount(v.x) && isCount(v.y) ? { x: v.x, y: v.y } : undefined;
}
function savePos(id: string, pos: GridPos) {
  try {
    localStorage.setItem(`pato:widget-pos:${id}`, JSON.stringify(pos));
  } catch {}
}

/** Axis-aligned overlap test between two tile rectangles. */
function overlaps(aPos: GridPos, aSize: TileSize, bPos: GridPos, bSize: TileSize): boolean {
  return (
    aPos.x < bPos.x + bSize.w &&
    aPos.x + aSize.w > bPos.x &&
    aPos.y < bPos.y + bSize.h &&
    aPos.y + aSize.h > bPos.y
  );
}

export class PatoGrid extends Component {
  #cells = new Map<string, Cell>();
  // Invisible real grid item spanning exactly one column, used only to
  // measure the true rendered column width (see #syncTile).
  #gauge = document.createElement("div");
  // One filler div per currently-empty cell in the used/visible extent (see
  // #syncFillers), keyed by "x,y".
  #fillers = new Map<string, HTMLDivElement>();
  #tileObserver?: ResizeObserver;
  #unsubscribeLock?: () => void;

  connectedCallback() {
    this.#gauge.className = "pato-tile-gauge";
    this.#gauge.style.gridColumn = "1 / span 1";
    this.#gauge.style.gridRow = "1 / span 1";
    this.append(this.#gauge);
    this.#tileObserver ??= new ResizeObserver(([entry]) => this.#syncTile(entry.contentRect.width));
    this.#tileObserver.observe(this.#gauge);
    // ResizeObserver's initial callback is async; read synchronously once so
    // --pato-grid-tile is correct before the first paint, not just eventually.
    this.#syncTile(this.#gauge.getBoundingClientRect().width);
    this.#unsubscribeLock ??= onLockChange((locked) => this.classList.toggle("locked", locked));
    this.addEventListener("scroll", this.#onScroll);
    this.#syncFillers();
  }

  disconnectedCallback() {
    this.#tileObserver?.disconnect();
    this.#unsubscribeLock?.();
    this.removeEventListener("scroll", this.#onScroll);
  }

  #onScroll = () => this.#syncFillers();

  /**
   * Publish the real, rendered pixel width of one column as `--pato-grid-tile`
   * so `grid-auto-rows` keeps rows square with columns. Measured directly off
   * `#gauge` - a real 1-column grid item - rather than recomputed from
   * container width/gap/scrollbar math.
   */
  #syncTile(width: number) {
    if (width <= 0) return;
    this.style.setProperty("--pato-grid-tile", `${width}px`);
    this.#syncFillers();
  }

  /** Rows tall enough to cover every placed widget. */
  #usedRows(): number {
    let max = 0;
    for (const cell of this.#cells.values()) max = Math.max(max, cell.pos.y + cell.size.h - 1);
    return max;
  }

  /** Rows tall enough to cover the currently scrolled-into-view area, plus one. */
  #visibleRows(): number {
    const tile = this.#tile();
    if (tile <= 0) return 0;
    const gap = parseFloat(getComputedStyle(this).rowGap) || 0;
    return Math.ceil((this.scrollTop + this.clientHeight) / (tile + gap)) + 1;
  }

  /**
   * Keep a filler div under every grid cell not covered by a real widget, up
   * to the used/visible row extent, so the container's line-coloured
   * background only ever shows through the true gaps between items (see the
   * `pato-grid` background-color comment in index.css). Not called from the
   * live drag/resize move handlers - only once a placement settles - since
   * recomputing the full occupancy grid on every pointermove would be wasted
   * work at 60fps.
   */
  #syncFillers() {
    const rows = Math.max(this.#usedRows(), this.#visibleRows());
    const wanted = new Set<string>();
    for (let y = 1; y <= rows; y++) {
      for (let x = 1; x <= GRID_COLS; x++) {
        if (this.#collides("", { x, y }, { w: 1, h: 1 })) continue;
        const key = `${x},${y}`;
        wanted.add(key);
        if (this.#fillers.has(key)) continue;
        const el = document.createElement("div");
        el.className = "pato-grid-filler";
        el.style.gridColumn = `${x} / span 1`;
        el.style.gridRow = `${y} / span 1`;
        this.append(el);
        this.#fillers.set(key, el);
      }
    }
    for (const [key, el] of this.#fillers) {
      if (wanted.has(key)) continue;
      el.remove();
      this.#fillers.delete(key);
    }
  }

  /**
   * The real per-column pixel width, as last published by `#syncTile`. Pointer
   * math must use this (not `rect.width / GRID_COLS`) - that naive division
   * ignores the 29 column gaps baked into the grid's width, overstates the
   * tile size, and undercounts grid units per pixel dragged by more and more
   * the further from the origin the pointer travels.
   */
  #tile(): number {
    return parseFloat(getComputedStyle(this).getPropertyValue("--pato-grid-tile")) || 0;
  }

  /** Reserve a grid cell for a widget and report its layout to the core. */
  registerWidget(spec: WidgetSpec) {
    const min = spec.min ?? spec.size;
    const max = spec.max ?? spec.size;
    const cell = this.#ensureCell(spec.id);
    cell.min = min;
    cell.max = max;
    const size = clampTiles(loadSize(spec.id) ?? spec.size, min, max);
    this.#place(spec.id, this.#restorePos(spec.id, size), size, false);
    this.#setResizeHandle(cell);
    this.#reportLayout(spec.id, size);
    this.#syncFillers();
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
    this.#syncFillers();
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

    const size = { w: 4, h: 4 };
    cell = { el, widget, pos: { x: 1, y: 1 }, size, min: { w: 1, h: 1 }, max: { w: GRID_COLS, h: GRID_COLS } };
    this.#cells.set(id, cell);
    cell.pos = this.#restorePos(id, size);
    this.#place(id, cell.pos, cell.size, false);
    this.#setDragHandle(cell);
    this.#syncFillers();
    return cell;
  }

  /**
   * Where a widget goes: its saved position if that still fits (inside the
   * columns and clear of every other widget - which stops holding when the
   * layout around it changed, e.g. a neighbour was removed and something else
   * took its place, or a plugin update resized a widget), otherwise the first
   * empty spot.
   */
  #restorePos(id: string, size: TileSize): GridPos {
    const saved = loadPos(id);
    if (saved && saved.x + size.w - 1 <= GRID_COLS && !this.#collides(id, saved, size)) return saved;
    return this.#firstFit(size, id);
  }

  /** First empty spot (row-major scan) a `size` footprint fits in, ignoring `excludeId`. */
  #firstFit(size: TileSize, excludeId: string): GridPos {
    for (let y = 1; ; y++) {
      for (let x = 1; x + size.w - 1 <= GRID_COLS; x++) {
        const pos = { x, y };
        if (!this.#collides(excludeId, pos, size)) return pos;
      }
    }
  }

  /** Whether `pos`/`size` overlaps any registered cell other than `excludeId`. */
  #collides(excludeId: string, pos: GridPos, size: TileSize): boolean {
    for (const [id, cell] of this.#cells) {
      if (id === excludeId) continue;
      if (overlaps(pos, size, cell.pos, cell.size)) return true;
    }
    return false;
  }

  #place(id: string, pos: GridPos, size: TileSize, persist: boolean) {
    const cell = this.#cells.get(id);
    if (!cell) return;
    cell.pos = pos;
    cell.size = size;
    cell.el.style.gridColumn = `${pos.x} / span ${size.w}`;
    cell.el.style.gridRow = `${pos.y} / span ${size.h}`;
    cell.el.style.aspectRatio = `${size.w} / ${size.h}`;
    if (persist) {
      savePos(id, pos);
      saveSize(id, size);
    }
  }

  #reportLayout(id: string, size: TileSize) {
    void callRust("widget_resized", { widgetId: id, w: size.w, h: size.h });
  }

  /** Add a corner drag handle when the widget is user-resizable. */
  #setResizeHandle(cell: Cell) {
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
      const tile = this.#tile();
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
        if (next.w !== cell.size.w || next.h !== cell.size.h) this.#place(cell.el.dataset.widgetId!, cell.pos, next, false);
      };
      const up = () => {
        handle.releasePointerCapture(down.pointerId);
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", up);
        const id = cell.el.dataset.widgetId!;
        this.#place(id, cell.pos, cell.size, true);
        this.#reportLayout(id, cell.size);
        this.#syncFillers();
      };
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
    });
    cell.el.append(handle);
  }

  /** Add a grip handle (visible only while the dashboard is unlocked) that repositions the widget. */
  #setDragHandle(cell: Cell) {
    const handle = document.createElement("div");
    handle.className = "pato-drag";
    handle.textContent = "⠿";
    handle.addEventListener("pointerdown", (down) => {
      down.preventDefault();
      handle.setPointerCapture(down.pointerId);
      const id = cell.el.dataset.widgetId!;
      const tile = this.#tile();
      const startPointer = { x: down.clientX, y: down.clientY };
      const startPos = { ...cell.pos };
      let candidate: GridPos = { ...startPos };
      cell.el.classList.add("dragging");

      const move = (m: PointerEvent) => {
        const dx = Math.round((m.clientX - startPointer.x) / tile);
        const dy = Math.round((m.clientY - startPointer.y) / tile);
        candidate = {
          x: Math.min(Math.max(1, startPos.x + dx), GRID_COLS - cell.size.w + 1),
          y: Math.max(1, startPos.y + dy),
        };
        this.#place(id, candidate, cell.size, false);
      };
      const up = () => {
        handle.releasePointerCapture(down.pointerId);
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", up);
        cell.el.classList.remove("dragging");
        // Dropping on an occupied spot snaps back to where the drag started.
        const target = this.#collides(id, candidate, cell.size) ? startPos : candidate;
        this.#place(id, target, cell.size, true);
        this.#syncFillers();
      };
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
    });
    cell.el.append(handle);
  }
}
export default component(PatoGrid);
