use crate::model::{DetailedStatus, RepoState, StatusKind, SyncState};
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn refresh_repo(name: &str, path: &Path, _timeout_sec: u64) -> RepoState {
    if !path.join(".git").exists() {
        return RepoState::new_error(
            name.to_string(),
            path.to_path_buf(),
            "repository unavailable".to_string(),
        );
    }

    let unstaged = match Command::new("git")
        .args(["diff", "--quiet"])
        .current_dir(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        Ok(s) => !s.success(),
        Err(e) => {
            return RepoState::new_error(name.to_string(), path.to_path_buf(), e.to_string());
        }
    };

    let has_untracked = match Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard", "--error-unmatch", "."])
        .current_dir(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        Ok(s) => s.success(),
        Err(_) => false,
    };
    let unstaged = unstaged || has_untracked;

    let staged = match Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        Ok(s) => !s.success(),
        Err(e) => {
            return RepoState::new_error(name.to_string(), path.to_path_buf(), e.to_string());
        }
    };

    let summary = match Command::new("git")
        .args(["log", "-1", "--format=%s"])
        .current_dir(path)
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => {
            return RepoState::new_error(
                name.to_string(),
                path.to_path_buf(),
                "no commits".to_string(),
            );
        }
    };

    let timestamp = match Command::new("git")
        .args(["log", "-1", "--format=%ct"])
        .current_dir(path)
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .trim()
            .parse::<i64>()
            .ok(),
        _ => None,
    };

    let status = match (staged, unstaged) {
        (true, true) => StatusKind::Both,
        (true, false) => StatusKind::StagedOnly,
        (false, true) => StatusKind::UnstagedOnly,
        (false, false) => StatusKind::Clean,
    };

    let sync = sync_state(path);

    RepoState {
        name: name.to_string(),
        path: path.to_path_buf(),
        status,
        commit_summary: summary,
        commit_timestamp: timestamp,
        error_msg: None,
        sync,
    }
}

pub fn sync_state(path: &Path) -> SyncState {
    let upstream_ok = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .current_dir(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !upstream_ok {
        return SyncState::default();
    }

    let output = Command::new("git")
        .args(["rev-list", "--left-right", "--count", "@{u}...HEAD"])
        .current_dir(path)
        .stderr(std::process::Stdio::null())
        .output();

    let (behind, ahead) = match output {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            let mut parts = s.split_whitespace();
            let b = parts.next().and_then(|x| x.parse().ok()).unwrap_or(0);
            let a = parts.next().and_then(|x| x.parse().ok()).unwrap_or(0);
            (b, a)
        }
        _ => (0, 0),
    };

    SyncState { has_upstream: true, ahead, behind }
}

pub fn fetch_repo(path: &Path) {
    let _ = Command::new("git")
        .args(["fetch", "--quiet", "--no-tags"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .current_dir(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

pub fn detailed_status(path: &Path) -> DetailedStatus {
    let parse_lines = |output: std::process::Output| -> Vec<String> {
        if output.status.success() {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect()
        } else {
            Vec::new()
        }
    };

    let staged = Command::new("git")
        .args(["diff", "--cached", "--name-status"])
        .current_dir(path)
        .stderr(std::process::Stdio::null())
        .output()
        .map(parse_lines)
        .unwrap_or_default();

    let unstaged = Command::new("git")
        .args(["diff", "--name-status"])
        .current_dir(path)
        .stderr(std::process::Stdio::null())
        .output()
        .map(parse_lines)
        .unwrap_or_default();

    let untracked = Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(path)
        .stderr(std::process::Stdio::null())
        .output()
        .map(parse_lines)
        .unwrap_or_default();

    DetailedStatus { staged, unstaged, untracked }
}

pub fn format_age(timestamp: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let diff = (now - timestamp).max(0);

    if diff < 60 {
        format!("{}s", diff)
    } else if diff < 3600 {
        format!("{}m", diff / 60)
    } else if diff < 86400 {
        format!("{}h", diff / 3600)
    } else {
        format!("{}d", diff / 86400)
    }
}
