//! Command line entry point and the terminal event loop.

use std::io::stdout;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use clap::Parser;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use remtui::app::App;
use remtui::bear::BearClient;
use remtui::client::{RemctlClient, remctl_found, resolve_remctl};
use remtui::config::{BearConfig, Config};

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

/// A sibling binary of this executable (the fakes ship next to `remtui`).
fn sibling(name: &str) -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    // Installed as a symlink in ~/bin: the fakes sit next to the real file.
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("no directory for {}", exe.display()))?;
    Ok(dir.join(name))
}

/// `--vim` > `REMTUI_KEYS=vim` > `[keys] profile = "vim"`.
pub fn vim_enabled(flag: bool, env_keys: Option<&str>, config: &Config) -> bool {
    flag || env_keys.is_some_and(|v| v.trim().eq_ignore_ascii_case("vim"))
        || config.profile == "vim"
}

/// Raw mode plus the alternate screen, undone on drop (also on panic).
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> anyhow::Result<TerminalGuard> {
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)?;
        Ok(TerminalGuard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(stdout(), DisableMouseCapture, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut config = Config::default();
    let loaded = remtui::config::load();
    config.profile = loaded.profile;
    config.overrides = loaded.overrides;
    config.bear = loaded.bear;
    let vim = vim_enabled(
        cli.vim,
        std::env::var("REMTUI_KEYS").ok().as_deref(),
        &config,
    );

    let (client, bear): (RemctlClient, Option<BearClient>) = if cli.demo {
        config.bear = BearConfig {
            enabled: true,
            list: "Work".into(),
            due: "today".into(),
            ..config.bear
        };
        (
            RemctlClient::new(vec![sibling("fake-remctl")?.to_string_lossy().into_owned()]),
            Some(BearClient::new(vec![
                sibling("fake-bearcli")?.to_string_lossy().into_owned(),
            ])),
        )
    } else {
        let binary = cli.remctl.clone().unwrap_or_else(resolve_remctl);
        if !remctl_found(&binary) {
            eprintln!(
                "remtui: '{binary}' not found on PATH.\nInstall remctl (https://github.com/viticci/remctl) and run 'remctl onboard', or try 'remtui --demo'."
            );
            std::process::exit(1);
        }
        (RemctlClient::new(vec![binary]), None)
    };

    let environ: std::collections::HashMap<String, String> = std::env::vars().collect();
    let (mut app, mut rx) = App::new(config, Arc::new(client), vim, environ);
    app.bear = bear.map(Arc::new);

    let mut guard = Some(TerminalGuard::enter()?);
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut events = EventStream::new();
    app.start();

    while app.running {
        terminal.draw(|frame| remtui::ui::draw(frame, &mut app))?;
        let deadline = app
            .next_deadline()
            .unwrap_or_else(|| Instant::now() + std::time::Duration::from_secs(3600));
        // The clock in the header ticks once a second.
        let clock = Instant::now() + std::time::Duration::from_secs(1);
        tokio::select! {
            event = events.next() => match event {
                Some(Ok(Event::Key(key))) => {
                    if key.kind == KeyEventKind::Press {
                        app.handle_key(key);
                    }
                }
                Some(Ok(Event::Mouse(mouse))) => app.handle_mouse(mouse),
                Some(Ok(_)) => {}
                Some(Err(err)) => return Err(err.into()),
                None => break,
            },
            msg = rx.recv() => match msg {
                Some(msg) => app.handle_msg(msg),
                None => break,
            },
            _ = tokio::time::sleep_until(deadline.min(clock).into()) => {}
        }
        while let Ok(msg) = rx.try_recv() {
            app.handle_msg(msg);
        }
        // An editor takes the terminal: leave the alternate screen, run it, come back.
        if let Some(job) = app.take_editor_job() {
            drop(guard.take());
            let outcome = remtui::editor::run(&job, false);
            guard = Some(TerminalGuard::enter()?);
            terminal.clear()?;
            app.editor_done(job, outcome);
        }
        app.tick(Instant::now());
    }
    drop(guard);
    Ok(())
}
