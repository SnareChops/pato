//! Modal dialogs and confirm prompts (`pato:plugin/modals`).
//!
//! The same trust rules as widgets apply: bodies go through `validate::tree`,
//! every id is namespaced with the calling plugin, and a plugin can only open,
//! update or close its own. On top of that the core caps how many modals one
//! plugin can have open, bounds the text it can put in the host-drawn chrome,
//! and labels every dialog with a plugin name it derives itself - the plugin
//! never chooses its own attribution.

use std::sync::LazyLock;

use dashmap::DashSet;
use wasmtime::component::Val;

use super::plugin::pato::plugin::modals;
use super::{split_id, validate, view, WIDGET_EVENTS};
use crate::js::call_js_or_default;
use crate::wasm;

/// Modals (and confirm prompts) one plugin may have open at once: enough for a
/// form plus a confirmation raised from it, not enough to bury the app.
const MAX_OPEN_PER_PLUGIN: usize = 2;
const MAX_TITLE: usize = 80;
const MAX_MESSAGE: usize = 1000;
const MAX_LABEL: usize = 30;

/// Namespaced ids (`"<plugin>|<local>"`) of every modal currently open. Also
/// the guard that keeps a plugin-initiated close and the webview's dismissal
/// report from both reaching the plugin.
static OPEN: LazyLock<DashSet<String>> = LazyLock::new(DashSet::new);

/// "pato_twitch" -> "Twitch": the name shown in a dialog's title bar.
fn plugin_label(name: &str) -> String {
    let base = name.strip_prefix("pato_").unwrap_or(name);
    let mut chars = base.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => name.to_string(),
    }
}

fn check_text(what: &str, value: &str, max: usize) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{what} is empty"));
    }
    if value.chars().count() > max {
        return Err(format!("{what} is longer than {max} characters"));
    }
    Ok(())
}

/// Claims a slot for `id`. Re-opening an id that is already showing is an
/// in-place update and never counts against the cap. Returns whether the id
/// was newly claimed (so the caller can release it if rendering fails), or
/// `None` if the plugin is at its cap.
fn reserve(plugin: &str, id: &str) -> Option<bool> {
    if OPEN.contains(id) {
        return Some(false);
    }
    let prefix = format!("{plugin}|");
    let open = OPEN.iter().filter(|k| k.starts_with(&prefix)).count();
    if open >= MAX_OPEN_PER_PLUGIN {
        return None;
    }
    OPEN.insert(id.to_string());
    Some(true)
}

impl modals::Host for wasm::HostData {
    async fn open(&mut self, modal: modals::Modal) -> bool {
        let checked = check_text("title", &modal.title, MAX_TITLE)
            .and_then(|_| validate::tree(modal.nodes, &self.name));
        let nodes = match checked {
            Ok(nodes) => nodes,
            Err(e) => {
                eprintln!("Rejected modal `{}` from {}: {e}", modal.id, self.name);
                return false;
            }
        };
        let id = format!("{}|{}", self.name, modal.id);
        let Some(claimed) = reserve(&self.name, &id) else {
            eprintln!(
                "Rejected modal `{}` from {}: too many open modals",
                modal.id, self.name
            );
            return false;
        };
        let view = view::Modal::Custom(view::CustomModal {
            id: id.clone(),
            plugin: plugin_label(&self.name),
            title: modal.title.trim().to_string(),
            size: match modal.size {
                modals::ModalSize::Sm => view::ModalSize::Sm,
                modals::ModalSize::Md => view::ModalSize::Md,
                modals::ModalSize::Lg => view::ModalSize::Lg,
            },
            nodes,
        });
        let shown = call_js_or_default::<_, bool>("modalOpen", view).await;
        if !shown && claimed {
            OPEN.remove(&id);
        }
        shown
    }

    async fn close(&mut self, id: String) {
        let id = format!("{}|{}", self.name, id);
        if OPEN.remove(&id).is_some() {
            call_js_or_default::<_, bool>("modalClose", id).await;
        }
    }

    async fn confirm(&mut self, prompt: modals::ConfirmPrompt) -> bool {
        let checked = check_text("title", &prompt.title, MAX_TITLE)
            .and_then(|_| check_text("message", &prompt.message, MAX_MESSAGE))
            .and_then(|_| check_text("confirm label", &prompt.confirm_label, MAX_LABEL));
        if let Err(e) = checked {
            eprintln!("Rejected confirm `{}` from {}: {e}", prompt.id, self.name);
            return false;
        }
        let id = format!("{}|{}", self.name, prompt.id);
        let Some(claimed) = reserve(&self.name, &id) else {
            eprintln!(
                "Rejected confirm `{}` from {}: too many open modals",
                prompt.id, self.name
            );
            return false;
        };
        let view = view::Modal::Confirm(view::ConfirmModal {
            id: id.clone(),
            plugin: plugin_label(&self.name),
            title: prompt.title.trim().to_string(),
            message: prompt.message,
            confirm_label: prompt.confirm_label.trim().to_string(),
            tone: match prompt.tone {
                modals::ConfirmTone::Normal => view::ConfirmTone::Normal,
                modals::ConfirmTone::Danger => view::ConfirmTone::Danger,
            },
        });
        let shown = call_js_or_default::<_, bool>("modalOpen", view).await;
        if !shown && claimed {
            OPEN.remove(&id);
        }
        shown
    }
}

// --- webview -> core: user dismissed / answered ------------------------------

/// The user closed a custom modal (close button, Esc or backdrop click).
#[tauri::command]
pub async fn modal_dismissed(id: String) -> Result<(), String> {
    if OPEN.remove(&id).is_none() {
        return Ok(());
    }
    let (plugin_name, local_id) = split_id(&id)?;
    wasm::plugin_call_by_name(
        &plugin_name,
        WIDGET_EVENTS,
        "on-modal-closed",
        vec![Val::String(local_id)],
        vec![],
    )
    .await?;
    Ok(())
}

/// The user answered a confirm prompt. Any way of dismissing it reports
/// `confirmed: false`.
#[tauri::command]
pub async fn confirm_answered(id: String, confirmed: bool) -> Result<(), String> {
    if OPEN.remove(&id).is_none() {
        return Ok(());
    }
    let (plugin_name, local_id) = split_id(&id)?;
    wasm::plugin_call_by_name(
        &plugin_name,
        WIDGET_EVENTS,
        "on-confirm",
        vec![Val::String(local_id), Val::Bool(confirmed)],
        vec![],
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_is_derived_from_plugin_name() {
        assert_eq!(plugin_label("pato_twitch"), "Twitch");
        assert_eq!(plugin_label("other"), "Other");
    }

    #[test]
    fn caps_open_modals_per_plugin_but_allows_updates() {
        // Unique plugin name so parallel tests don't share state.
        let p = "captest";
        assert_eq!(reserve(p, "captest|a"), Some(true));
        assert_eq!(reserve(p, "captest|a"), Some(false));
        assert_eq!(reserve(p, "captest|b"), Some(true));
        assert_eq!(reserve(p, "captest|c"), None);
        // Another plugin is unaffected.
        assert_eq!(reserve("captest2", "captest2|a"), Some(true));
        for id in ["captest|a", "captest|b", "captest2|a"] {
            OPEN.remove(id);
        }
        assert_eq!(reserve(p, "captest|c"), Some(true));
        OPEN.remove("captest|c");
    }

    #[test]
    fn text_bounds() {
        assert!(check_text("t", "  ", 5).is_err());
        assert!(check_text("t", "abcdef", 5).is_err());
        assert!(check_text("t", "abcde", 5).is_ok());
    }
}
