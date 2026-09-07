// One file per widget; this module re-exports each so callers use
// `widgets::TwitchStatusWidget`, not `widgets::status::TwitchStatusWidget`.
mod status;

pub use status::TwitchStatusWidget;
