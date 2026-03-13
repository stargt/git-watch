mod config;
mod git;
mod model;
mod ui;
mod watcher;

use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal;
use model::Message;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "git-watch", about = "Monitor multiple Git repositories")]
struct Args {
    /// Path to config file
    #[arg(short, long, default_value = "config.yml")]
    config: PathBuf,
}

fn main() {
    let args = Args::parse();

    let cfg = match config::Config::load(&args.config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    // Build initial repo states
    let mut repos: Vec<model::RepoState> = cfg
        .all_repos()
        .into_iter()
        .map(|(name, path, valid)| {
            if valid {
                git::refresh_repo(&name, &path, cfg.git.command_timeout_sec)
            } else {
                model::RepoState::new_error(name, path, "repository unavailable".to_string())
            }
        })
        .collect();

    // Set up message channel
    let (tx, rx) = mpsc::channel::<Message>();

    // Collect valid repo paths for watching
    let watch_paths: Vec<PathBuf> = repos
        .iter()
        .filter(|r| r.error_msg.is_none())
        .map(|r| r.path.clone())
        .collect();

    // Start filesystem watcher
    let _watcher = match watcher::start_watcher(watch_paths, cfg.watch.debounce_ms, tx.clone()) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Failed to start watcher: {}", e);
            std::process::exit(1);
        }
    };

    // Start reconciliation timer
    watcher::start_reconciliation(cfg.watch.reconcile_interval_sec, tx.clone());

    // Enter terminal UI
    if let Err(e) = ui::enter_ui() {
        eprintln!("Failed to initialize UI: {}", e);
        std::process::exit(1);
    }

    // Install panic hook to restore terminal
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = ui::exit_ui();
        orig_hook(info);
    }));

    // Use actual terminal width, falling back to config
    let mut width = terminal::size().map(|(cols, _)| cols as usize).unwrap_or(cfg.ui.width);

    // Initial render
    ui::render(
        &repos,
        width,
        cfg.ui.color,
        cfg.ui.show_clean,
        cfg.ui.blank_line_between_repos,
    );

    // Event loop
    loop {
        // Poll for terminal events
        if event::poll(Duration::from_millis(100)).unwrap_or(false) {
            match event::read() {
                Ok(Event::Key(key)) => match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Char('r') => {
                        // Force refresh all
                        for repo in &mut repos {
                            *repo = git::refresh_repo(
                                &repo.name,
                                &repo.path,
                                cfg.git.command_timeout_sec,
                            );
                        }
                        ui::render(
                            &repos,
                            width,
                            cfg.ui.color,
                            cfg.ui.show_clean,
                            cfg.ui.blank_line_between_repos,
                        );
                    }
                    _ => {}
                },
                Ok(Event::Resize(cols, _rows)) => {
                    width = cols as usize;
                    ui::render(
                        &repos,
                        width,
                        cfg.ui.color,
                        cfg.ui.show_clean,
                        cfg.ui.blank_line_between_repos,
                    );
                }
                _ => {}
            }
        }

        // Drain watcher messages
        let mut changed = false;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Message::RepoChanged(path) => {
                    if let Some(repo) = repos.iter_mut().find(|r| r.path == path) {
                        *repo = git::refresh_repo(
                            &repo.name,
                            &repo.path,
                            cfg.git.command_timeout_sec,
                        );
                        changed = true;
                    }
                }
                Message::ReconcileAll => {
                    for repo in &mut repos {
                        *repo = git::refresh_repo(
                            &repo.name,
                            &repo.path,
                            cfg.git.command_timeout_sec,
                        );
                    }
                    changed = true;
                }
                Message::Quit => break,
            }
        }

        if changed {
            ui::render(
                &repos,
                width,
                cfg.ui.color,
                cfg.ui.show_clean,
                cfg.ui.blank_line_between_repos,
            );
        }
    }

    let _ = ui::exit_ui();
}
