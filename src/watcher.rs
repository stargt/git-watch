use crate::model::Message;
use notify::{recommended_watcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
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
