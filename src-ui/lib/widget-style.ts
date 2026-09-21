import type { Style, Size, Align, Justify } from "pato:internal/widget-dom@0.1.0";

/**
 * Base stylesheet injected into every <pato-custom-widget> shadow root. The
 * palette / spacing / type tokens live on :root in index.css and inherit
 * across the shadow boundary, so this only carries the reset and host box.
 */
export const WIDGET_BASE_CSS = `
  :host {
    display: block;
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    overflow: clip;
    contain: content;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    font-size: var(--pato-text-md);
    color: var(--pato-color-fg);
    background: var(--pato-color-bg-raised);
    border: 1px solid var(--pato-color-border);
    border-radius: var(--pato-radius-lg);
  }
  * { box-sizing: border-box; }
  .pato-root { width: 100%; height: 100%; padding: var(--pato-space-md); overflow: auto; }
  button {
    font: inherit; color: inherit; cursor: pointer;
    background: var(--pato-color-bg); border: 1px solid var(--pato-color-border);
    border-radius: var(--pato-radius-md); padding: var(--pato-space-sm) var(--pato-space-md);
  }
  button:disabled { opacity: 0.5; cursor: default; }
  input, textarea, select {
    font: inherit; color: inherit; background: var(--pato-color-bg);
    border: 1px solid var(--pato-color-border); border-radius: var(--pato-radius-sm);
    padding: var(--pato-space-xs) var(--pato-space-sm);
  }
  a { color: var(--pato-color-primary); }
  img { max-width: 100%; }
  .pato-resize {
    position: absolute; right: 0; bottom: 0; width: 14px; height: 14px;
    cursor: nwse-resize; touch-action: none;
    background: linear-gradient(135deg, transparent 45%, var(--pato-color-fg-muted) 45% 55%, transparent 55%);
    opacity: 0.5;
  }
  .pato-resize:hover { opacity: 1; }
`;

const FLEX_ALIGN: Record<Align, string> = {
  start: "flex-start",
  center: "center",
  end: "flex-end",
  stretch: "stretch",
};
const FLEX_JUSTIFY: Record<Justify, string> = {
  start: "flex-start",
  center: "center",
  end: "flex-end",
  between: "space-between",
  around: "space-around",
};

function sizeToCss(size: Size): string {
  switch (size.tag) {
    case "auto":
      return "auto";
    case "fill":
      return "100%";
    case "percent":
      return `${Math.max(0, Math.min(100, size.val))}%`;
    case "fixed":
      return `var(--pato-space-${size.val})`;
  }
}

/**
 * Maps a structured widget `Style` to a CSS text string. Only known
 * properties with token-backed values are emitted — never a raw string from
 * the plugin.
 */
export function styleToCss(style: Style | undefined): string {
  if (!style) return "";
  const out: string[] = [];

  if (style.layout === "flex-row") out.push("display:flex", "flex-direction:row");
  if (style.layout === "flex-col") out.push("display:flex", "flex-direction:column");

  if (style.gap) out.push(`gap:var(--pato-space-${style.gap})`);
  if (style.padding) out.push(`padding:var(--pato-space-${style.padding})`);
  if (style.margin) out.push(`margin:var(--pato-space-${style.margin})`);
  if (style.radius) out.push(`border-radius:var(--pato-radius-${style.radius})`);
  if (style.color) out.push(`color:var(--pato-color-${style.color})`);
  if (style.background) out.push(`background:var(--pato-color-${style.background})`);
  if (style.fontSize) out.push(`font-size:var(--pato-text-${style.fontSize})`);
  if (style.align) out.push(`align-items:${FLEX_ALIGN[style.align]}`);
  if (style.justify) out.push(`justify-content:${FLEX_JUSTIFY[style.justify]}`);
  if (style.grow !== undefined) out.push(`flex-grow:${style.grow ? 1 : 0}`);
  if (style.wrap) out.push("flex-wrap:wrap");
  if (style.width) out.push(`width:${sizeToCss(style.width)}`);
  if (style.height) out.push(`height:${sizeToCss(style.height)}`);
  // `scroll` needs `min-height:0`/`min-width:0` alongside it or a flex child
  // never shrinks below its content size and the scrollbar never appears.
  if (style.overflow === "scroll") out.push("overflow:auto", "min-height:0", "min-width:0");
  if (style.overflow === "clip") out.push("overflow:clip");

  return out.join(";");
}
