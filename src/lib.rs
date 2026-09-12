//! remtui: a terminal front end for Apple Reminders, talking to Reminders only
//! through `remctl`, and to Bear through `bearcli` for todo triage.
//!
//! Module names mirror the Python implementation (`~/GitHub/remtui`) so the two
//! trees can be read side by side.

pub mod bear;
pub mod client;
pub mod config;
pub mod dates;
pub mod fake;
pub mod keys;
pub mod models;
pub mod todos;
pub mod triage;
pub mod util;
