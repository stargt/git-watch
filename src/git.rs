use crate::model::{RepoState, StatusKind};
use std::path::Path;
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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

    RepoState {
        name: name.to_string(),
        path: path.to_path_buf(),
        status,
        commit_summary: summary,
        commit_timestamp: timestamp,
        last_refresh: Instant::now(),
        error_msg: None,
    }
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
