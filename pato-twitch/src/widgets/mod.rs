// One file per widget; this module re-exports each so callers use
// `widgets::TwitchStatusWidget`, not `widgets::status::TwitchStatusWidget`.
mod panel;
mod status;

pub use panel::{TwitchPanel, HANDLER_REFRESH};
pub use status::TwitchStatusWidget;
