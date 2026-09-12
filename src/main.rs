//! Command line entry point. The terminal loop lands in Phase 8.

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "remtui",
    about = "A terminal front end for Apple Reminders, powered by remctl."
)]
struct Cli {
    /// run against a bundled fake reminders store (no remctl needed)
    #[arg(long)]
    demo: bool,
    /// path to the remctl binary (default: $REMTUI_REMCTL or 'remctl' on PATH)
    #[arg(long, value_name = "PATH")]
    remctl: Option<String>,
    /// enable the vim key profile (gg/G, ctrl+d/u/f/b, :, o); also enabled via REMTUI_KEYS=vim or the config file
    #[arg(long)]
    vim: bool,
}

fn main() -> anyhow::Result<()> {
    let _cli = Cli::parse();
    anyhow::bail!("the TUI lands in Phase 8")
}
