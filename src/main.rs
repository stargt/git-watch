mod config;
mod git;
mod model;
mod ui;
mod watcher;

use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal;
use model::{Message, OpKind, StatusKind, ViewMode};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "git-watch", about = "Monitor multiple Git repositories")]
struct Args {
    /// Path to config file
    #[arg(short, long, default_value = "config.yml")]
    config: PathBuf,
}

fn is_visible(repo: &model::RepoState, show_clean: bool) -> bool {
    show_clean || !matches!(repo.status, StatusKind::Clean)
}

fn first_visible(repos: &[model::RepoState], show_clean: bool) -> Option<usize> {
    repos.iter().position(|r| is_visible(r, show_clean))
}

fn next_visible(repos: &[model::RepoState], current: usize, show_clean: bool) -> Option<usize> {
    (current + 1..repos.len()).find(|&i| is_visible(&repos[i], show_clean))
}

fn prev_visible(repos: &[model::RepoState], current: usize, show_clean: bool) -> Option<usize> {
    (0..current).rev().find(|&i| is_visible(&repos[i], show_clean))
}

fn watch_paths_clone(repos: &[model::RepoState]) -> Vec<PathBuf> {
    repos
        .iter()
        .filter(|r| r.error_msg.is_none())
        .map(|r| r.path.clone())
        .collect()
}

fn spawn_op(tx: &mpsc::Sender<Message>, repo: &model::RepoState, kind: OpKind) {
    let tx = tx.clone();
    let path = repo.path.clone();
    thread::spawn(move || {
        let result = match kind {
            OpKind::Push => git::push_repo(&path),
            OpKind::Pull => git::pull_repo(&path),
        };
        let _ = tx.send(Message::OpFinished {
            repo: path,
            kind,
            result,
        });
    });
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
    let watch_paths: Vec<PathBuf> = watch_paths_clone(&repos);

    // Start filesystem watcher
    let _watcher = match watcher::start_watcher(watch_paths.clone(), cfg.watch.debounce_ms, tx.clone()) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Failed to start watcher: {}", e);
            std::process::exit(1);
        }
    };

    // Start reconciliation timer
    watcher::start_reconciliation(cfg.watch.reconcile_interval_sec, tx.clone());

    // Start periodic fetcher (also handles manual triggers via fetch_trigger.trigger())
    let fetch_trigger = watcher::start_fetcher(
        watch_paths,
        cfg.watch.fetch_interval_sec,
        tx.clone(),
    );

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

    let mut selected: usize = first_visible(&repos, cfg.ui.show_clean).unwrap_or(0);
    let mut view = ViewMode::List;
    let mut is_fetching = false;
    let mut op_status: Option<String> = None;
    let mut op_in_flight: Option<(PathBuf, OpKind)> = None;

    let render_current = |repos: &[model::RepoState],
                          view: &ViewMode,
                          width,
                          selected,
                          is_fetching: bool,
                          op_status: Option<&str>| {
        match view {
            ViewMode::List => ui::render(
                repos,
                width,
                cfg.ui.color,
                cfg.ui.show_clean,
                cfg.ui.blank_line_between_repos,
                selected,
                cfg.ui.compact_threshold,
                is_fetching,
                op_status,
            ),
            ViewMode::Detail { index, ref status } => {
                ui::render_detail(&repos[*index], status, width, cfg.ui.color, op_status);
            }
        }
    };

    // Initial render
    render_current(&repos, &view, width, selected, is_fetching, op_status.as_deref());

    // Event loop
    loop {
        // Poll for terminal events
        if event::poll(Duration::from_millis(100)).unwrap_or(false) {
            match event::read() {
                Ok(Event::Key(key)) => {
                    // Ctrl+C always quits
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        break;
                    }

                    let cleared = if op_in_flight.is_none() {
                        op_status.take().is_some()
                    } else {
                        false
                    };

                    match &view {
                        ViewMode::List => match key.code {
                            KeyCode::Char('q') => break,
                            KeyCode::Up | KeyCode::Char('k') => {
                                if let Some(idx) =
                                    prev_visible(&repos, selected, cfg.ui.show_clean)
                                {
                                    selected = idx;
                                    render_current(
                                        &repos,
                                        &view,
                                        width,
                                        selected,
                                        is_fetching,
                                        op_status.as_deref(),
                                    );
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                if let Some(idx) =
                                    next_visible(&repos, selected, cfg.ui.show_clean)
                                {
                                    selected = idx;
                                    render_current(
                                        &repos,
                                        &view,
                                        width,
                                        selected,
                                        is_fetching,
                                        op_status.as_deref(),
                                    );
                                }
                            }
                            KeyCode::Enter => {
                                if selected < repos.len() {
                                    let detail =
                                        git::detailed_status(&repos[selected].path);
                                    view = ViewMode::Detail {
                                        index: selected,
                                        status: detail,
                                    };
                                    render_current(
                                        &repos,
                                        &view,
                                        width,
                                        selected,
                                        is_fetching,
                                        op_status.as_deref(),
                                    );
                                }
                            }
                            KeyCode::Char('r') => {
                                for repo in &mut repos {
                                    *repo = git::refresh_repo(
                                        &repo.name,
                                        &repo.path,
                                        cfg.git.command_timeout_sec,
                                    );
                                }
                                render_current(
                                    &repos,
                                    &view,
                                    width,
                                    selected,
                                    is_fetching,
                                    op_status.as_deref(),
                                );
                            }
                            KeyCode::Char('f') => {
                                fetch_trigger.trigger();
                            }
                            KeyCode::Char('p') | KeyCode::Char('l') => {
                                let kind = if matches!(key.code, KeyCode::Char('p')) {
                                    OpKind::Push
                                } else {
                                    OpKind::Pull
                                };
                                if op_in_flight.is_none()
                                    && selected < repos.len()
                                    && repos[selected].error_msg.is_none()
                                {
                                    let r = &repos[selected];
                                    op_in_flight = Some((r.path.clone(), kind));
                                    op_status = Some(match kind {
                                        OpKind::Push => format!("pushing {}...", r.name),
                                        OpKind::Pull => format!("pulling {}...", r.name),
                                    });
                                    spawn_op(&tx, r, kind);
                                    render_current(
                                        &repos,
                                        &view,
                                        width,
                                        selected,
                                        is_fetching,
                                        op_status.as_deref(),
                                    );
                                }
                            }
                            _ => {}
                        },
                        ViewMode::Detail { index, .. } => match key.code {
                            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Left => {
                                view = ViewMode::List;
                                render_current(
                                    &repos,
                                    &view,
                                    width,
                                    selected,
                                    is_fetching,
                                    op_status.as_deref(),
                                );
                            }
                            KeyCode::Char('r') => {
                                let idx = *index;
                                repos[idx] = git::refresh_repo(
                                    &repos[idx].name,
                                    &repos[idx].path,
                                    cfg.git.command_timeout_sec,
                                );
                                let detail =
                                    git::detailed_status(&repos[idx].path);
                                view = ViewMode::Detail {
                                    index: idx,
                                    status: detail,
                                };
                                render_current(
                                    &repos,
                                    &view,
                                    width,
                                    selected,
                                    is_fetching,
                                    op_status.as_deref(),
                                );
                            }
                            KeyCode::Char('f') => {
                                fetch_trigger.trigger();
                            }
                            KeyCode::Char('p') | KeyCode::Char('l') => {
                                let kind = if matches!(key.code, KeyCode::Char('p')) {
                                    OpKind::Push
                                } else {
                                    OpKind::Pull
                                };
                                let idx = *index;
                                if op_in_flight.is_none()
                                    && idx < repos.len()
                                    && repos[idx].error_msg.is_none()
                                {
                                    let r = &repos[idx];
                                    op_in_flight = Some((r.path.clone(), kind));
                                    op_status = Some(match kind {
                                        OpKind::Push => format!("pushing {}...", r.name),
                                        OpKind::Pull => format!("pulling {}...", r.name),
                                    });
                                    spawn_op(&tx, r, kind);
                                    render_current(
                                        &repos,
                                        &view,
                                        width,
                                        selected,
                                        is_fetching,
                                        op_status.as_deref(),
                                    );
                                }
                            }
                            _ => {}
                        },
                    }

                    if cleared && op_status.is_none() {
                        render_current(
                            &repos,
                            &view,
                            width,
                            selected,
                            is_fetching,
                            op_status.as_deref(),
                        );
                    }
                }
                Ok(Event::Resize(cols, _rows)) => {
                    width = cols as usize;
                    render_current(
                        &repos,
                        &view,
                        width,
                        selected,
                        is_fetching,
                        op_status.as_deref(),
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
                Message::FetchStarted => {
                    is_fetching = true;
                    changed = true;
                }
                Message::FetchFinished => {
                    is_fetching = false;
                    changed = true;
                }
                Message::OpFinished { repo, kind, result } => {
                    if let Some((p, k)) = &op_in_flight {
                        if p == &repo && *k == kind {
                            op_in_flight = None;
                        }
                    }
                    if let Some(rs) = repos.iter_mut().find(|r| r.path == repo) {
                        let name = rs.name.clone();
                        *rs = git::refresh_repo(
                            &name,
                            &repo,
                            cfg.git.command_timeout_sec,
                        );
                        op_status = Some(match (kind, result) {
                            (OpKind::Push, Ok(())) => format!("pushed: {}", name),
                            (OpKind::Push, Err(e)) => format!("push failed: {}", e),
                            (OpKind::Pull, Ok(())) => format!("pulled: {}", name),
                            (OpKind::Pull, Err(e)) => format!("pull failed: {}", e),
                        });
                    }
                    changed = true;
                }
            }
        }

        if changed {
            // If in detail view and the viewed repo changed, refresh detail too
            if let ViewMode::Detail { index, .. } = &view {
                let idx = *index;
                let detail = git::detailed_status(&repos[idx].path);
                view = ViewMode::Detail {
                    index: idx,
                    status: detail,
                };
            }
            render_current(
                &repos,
                &view,
                width,
                selected,
                is_fetching,
                op_status.as_deref(),
            );
        }
    }

    let _ = ui::exit_ui();
}
