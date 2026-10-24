import { bind, callRust } from "./bindings.js";
import { PatoCustomWidget } from "../widgets/custom-widget.js";
import type { ConfirmModal, CustomModal, Modal } from "pato:internal/modal-view@0.1.0";

// Signature and payload shapes are checked against wit-internal/core-ui.wit
// (`modal-view`) via the jco-generated types in src-ui/generated/.
//
// Every modal is a native <dialog> opened with showModal(): it lives in the
// browser's top layer (so nothing a widget does can paint over it), makes the
// rest of the page inert, traps focus, and restores focus when closed. All the
// chrome is built here from plain strings with textContent - the only
// plugin-authored content is the node table rendered by <pato-custom-widget>.

type Entry = {
  dialog: HTMLDialogElement;
  kind: "custom" | "confirm";
  title: HTMLElement;
  widget?: PatoCustomWidget;
};

const modals = new Map<string, Entry>();

function make<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, text?: string): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  el.className = className;
  if (text !== undefined) el.textContent = text;
  return el;
}

function closeLocal(id: string) {
  const entry = modals.get(id);
  if (!entry) return;
  modals.delete(id);
  entry.dialog.close();
  entry.dialog.remove();
}

/** Builds the dialog shell: host-drawn header (plugin badge, title, close) plus an empty body. */
function shell(
  id: string,
  plugin: string,
  title: string,
  sizeClass: string,
  onDismiss: () => void,
): { dialog: HTMLDialogElement; title: HTMLElement; body: HTMLElement } {
  const dialog = make("dialog", `pato-modal ${sizeClass}`);
  const panel = make("div", "pato-modal-panel");
  const header = make("div", "pato-modal-header");
  const badge = make("span", "pato-modal-plugin", plugin);
  const titleEl = make("h2", "pato-modal-title", title);
  titleEl.id = `pato-modal-title-${modals.size}-${Math.random().toString(36).slice(2)}`;
  dialog.setAttribute("aria-labelledby", titleEl.id);
  const close = make("button", "pato-modal-close", "×");
  close.type = "button";
  close.setAttribute("aria-label", "Close");
  close.addEventListener("click", onDismiss);
  header.append(badge, titleEl, close);
  const body = make("div", "pato-modal-body");
  panel.append(header, body);
  dialog.append(panel);

  // Esc: the browser would close the dialog itself; route it through the same
  // dismissal path as the close button so the core hears about it exactly once.
  dialog.addEventListener("cancel", (ev) => {
    ev.preventDefault();
    onDismiss();
  });
  // Backdrop click. The panel fills the dialog, so a click whose target is the
  // dialog element itself landed on the backdrop. Requiring the press to start
  // there too avoids dismissing when a text selection drag ends outside.
  let pressedBackdrop = false;
  dialog.addEventListener("pointerdown", (ev) => (pressedBackdrop = ev.target === dialog));
  dialog.addEventListener("click", (ev) => {
    if (pressedBackdrop && ev.target === dialog) onDismiss();
    pressedBackdrop = false;
  });

  document.body.append(dialog);
  return { dialog, title: titleEl, body };
}

function openCustom(m: CustomModal): boolean {
  const existing = modals.get(m.id);
  if (existing?.kind === "custom" && existing.widget) {
    existing.title.textContent = m.title;
    existing.dialog.className = `pato-modal pato-modal-${m.size}`;
    existing.widget.render(m.nodes);
    return true;
  }
  if (existing) closeLocal(m.id);

  const dismiss = () => {
    if (!modals.has(m.id)) return;
    closeLocal(m.id);
    void callRust("modal_dismissed", { id: m.id });
  };
  const { dialog, title, body } = shell(m.id, m.plugin, m.title, `pato-modal-${m.size}`, dismiss);
  const widget = new PatoCustomWidget();
  widget.widgetId = m.id;
  widget.setAttribute("modal", "");
  widget.render(m.nodes);
  body.append(widget);
  modals.set(m.id, { dialog, kind: "custom", title, widget });
  dialog.showModal();
  // Land the cursor in the form rather than on the header's close button.
  widget.shadowRoot?.querySelector<HTMLElement>("input, textarea, select, button")?.focus();
  return true;
}

function openConfirm(m: ConfirmModal): boolean {
  if (modals.has(m.id)) closeLocal(m.id);

  const answer = (confirmed: boolean) => {
    if (!modals.has(m.id)) return;
    closeLocal(m.id);
    void callRust("confirm_answered", { id: m.id, confirmed });
  };
  const { dialog, title, body } = shell(m.id, m.plugin, m.title, "pato-modal-sm", () => answer(false));
  dialog.setAttribute("role", "alertdialog");
  body.append(make("p", "pato-modal-message", m.message));

  const actions = make("div", "pato-modal-actions");
  const cancel = make("button", "pato-modal-btn", "Cancel");
  cancel.type = "button";
  cancel.addEventListener("click", () => answer(false));
  const confirm = make("button", `pato-modal-btn ${m.tone === "danger" ? "danger" : "primary"}`, m.confirmLabel);
  confirm.type = "button";
  confirm.addEventListener("click", () => answer(true));
  // A destructive prompt opens on Cancel so a stray Enter can't confirm it.
  (m.tone === "danger" ? cancel : confirm).autofocus = true;
  actions.append(cancel, confirm);
  body.append(actions);

  modals.set(m.id, { dialog, kind: "confirm", title });
  dialog.showModal();
  return true;
}

bind(function modalOpen(modal: Modal): boolean {
  switch (modal.tag) {
    case "custom":
      return openCustom(modal.val);
    case "confirm":
      return openConfirm(modal.val);
    default: {
      const _: never = modal;
      return false;
    }
  }
});

bind(function modalClose(id: string): boolean {
  closeLocal(id);
  return true;
});
