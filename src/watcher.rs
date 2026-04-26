use crate::git;
use crate::model::Message;
use notify::{recommended_watcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

fn find_repo(event_path: &Path, repo_paths: &[PathBuf]) -> Option<PathBuf> {
    repo_paths
        .iter()
        .filter(|rp| event_path.starts_with(rp))
        .max_by_key(|rp| rp.as_os_str().len())
        .cloned()
}

fn should_ignore(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    path_str.contains(".git/objects/") || path_str.contains(".git/logs/")
}

pub fn start_watcher(
    repo_paths: Vec<PathBuf>,
    debounce_ms: u64,
    tx: Sender<Message>,
) -> notify::Result<impl Watcher> {
    let last_sent: Arc<Mutex<HashMap<PathBuf, Instant>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let debounce_dur = Duration::from_millis(debounce_ms);

    let paths = repo_paths.clone();
    let mut watcher = recommended_watcher(move |res: notify::Result<notify::Event>| {
        let event = match res {
            Ok(e) => e,
            Err(_) => return,
        };

        for event_path in &event.paths {
            if should_ignore(event_path) {
                continue;
            }

            let repo_path = match find_repo(event_path, &paths) {
                Some(p) => p,
                None => continue,
            };

            let mut map = last_sent.lock().unwrap();
            let now = Instant::now();
            let should_send = match map.get(&repo_path) {
                Some(last) => now.duration_since(*last) >= debounce_dur,
                None => true,
            };

            if should_send {
                map.insert(repo_path.clone(), now);
                let _ = tx.send(Message::RepoChanged(repo_path));
            } else {
                // Schedule a delayed send for end-of-burst
                let tx2 = tx.clone();
                let repo2 = repo_path.clone();
                let last_sent2 = Arc::clone(&last_sent);
                let debounce = debounce_dur;
                let entry_time = *map.get(&repo_path).unwrap();

                thread::spawn(move || {
                    thread::sleep(debounce);
                    let mut map = last_sent2.lock().unwrap();
                    // Only send if no newer event has already sent
                    if let Some(last) = map.get(&repo2) {
                        if *last == entry_time {
                            map.insert(repo2.clone(), Instant::now());
                            let _ = tx2.send(Message::RepoChanged(repo2));
                        }
                    }
                });
            }
        }
    })?;

    for path in &repo_paths {
        if path.exists() {
            watcher.watch(path, RecursiveMode::Recursive)?;
        }
    }

    Ok(watcher)
}

pub fn start_reconciliation(interval_sec: u64, tx: Sender<Message>) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(interval_sec));
        let _ = tx.send(Message::ReconcileAll);
    });
}

pub struct FetchTrigger {
    sender: Sender<()>,
}

impl FetchTrigger {
    pub fn trigger(&self) {
        let _ = self.sender.send(());
    }
}

pub fn start_fetcher(
    repo_paths: Vec<PathBuf>,
    interval_sec: u64,
    tx: Sender<Message>,
) -> FetchTrigger {
    let (trigger_tx, trigger_rx) = mpsc::channel::<()>();

    thread::spawn(move || {
        let interval = Duration::from_secs(interval_sec);
        loop {
            // Wait for either timer expiry or manual trigger.
            // Both paths fall through to the fetch step below.
            let _ = trigger_rx.recv_timeout(interval);
            // Drain extra triggers so multiple presses collapse into one fetch.
            while trigger_rx.try_recv().is_ok() {}

            let _ = tx.send(Message::FetchStarted);
            for path in &repo_paths {
                git::fetch_repo(path);
            }
            let _ = tx.send(Message::FetchFinished);
            let _ = tx.send(Message::ReconcileAll);
        }
    });

    FetchTrigger { sender: trigger_tx }
}
