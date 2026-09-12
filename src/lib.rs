//! remtui: a terminal front end for Apple Reminders, talking to Reminders only
//! through `remctl`, and to Bear through `bearcli` for todo triage.
//!
//! Module names mirror the Python implementation (`~/GitHub/remtui-python`, archived) so the two
//! trees can be read side by side.

pub mod app;
pub mod bear;
pub mod client;
pub mod config;
pub mod dates;
pub mod editor;
pub mod fake;
pub mod harness;
pub mod keys;
pub mod models;
pub mod todos;
pub mod triage;
pub mod ui;
pub mod util;
