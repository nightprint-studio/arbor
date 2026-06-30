//! OAuth2 flows.
//!
//! Each flow is a small config struct + a couple of methods. Provider
//! differences live in the config, not in different impl types — this
//! keeps the crate small and avoids needing trait polymorphism for the
//! handful of flows we actually support.

mod device_flow;
mod installed_app;
mod refresh;

pub use device_flow::{DeviceFlow, PollOutcome};
pub use installed_app::{InstalledAppFlow, PendingAuth};
pub use refresh::refresh_token;
