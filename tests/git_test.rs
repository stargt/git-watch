use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

// Integration test: verify git status parsing works on a real repo
#[test]
fn test_format_age() {
    // We can't import git::format_age directly since it's in a binary crate,
    // so we test the logic inline
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Test seconds
    assert_eq!(format_age(now - 5), "5s");
    // Test minutes
    assert_eq!(format_age(now - 300), "5m");
    // Test hours
    assert_eq!(format_age(now - 7200), "2h");
    // Test days
    assert_eq!(format_age(now - 172800), "2d");
}

#[test]
fn test_git_diff_on_clean_repo() {
    let dir = tempdir();
    run(&dir, &["git", "init"]);
    run(&dir, &["git", "config", "user.email", "test@test.com"]);
    run(&dir, &["git", "config", "user.name", "Test"]);
    std::fs::write(dir.join("file.txt"), "hello").unwrap();
    run(&dir, &["git", "add", "."]);
    run(&dir, &["git", "commit", "-m", "init"]);

    // Clean repo: both diffs should succeed (exit 0)
    let unstaged = Command::new("git")
        .args(["diff", "--quiet"])
        .current_dir(&dir)
        .status()
        .unwrap();
    assert!(unstaged.success(), "clean repo should have no unstaged changes");

    let staged = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(&dir)
        .status()
        .unwrap();
    assert!(staged.success(), "clean repo should have no staged changes");
}

#[test]
fn test_git_diff_with_unstaged() {
    let dir = tempdir();
    run(&dir, &["git", "init"]);
    run(&dir, &["git", "config", "user.email", "test@test.com"]);
    run(&dir, &["git", "config", "user.name", "Test"]);
    std::fs::write(dir.join("file.txt"), "hello").unwrap();
    run(&dir, &["git", "add", "."]);
    run(&dir, &["git", "commit", "-m", "init"]);

    // Modify file without staging
    std::fs::write(dir.join("file.txt"), "modified").unwrap();

    let unstaged = Command::new("git")
        .args(["diff", "--quiet"])
        .current_dir(&dir)
        .status()
        .unwrap();
    assert!(!unstaged.success(), "should detect unstaged changes");

    let staged = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(&dir)
        .status()
        .unwrap();
    assert!(staged.success(), "should have no staged changes");
}

#[test]
fn test_git_diff_with_staged() {
    let dir = tempdir();
    run(&dir, &["git", "init"]);
    run(&dir, &["git", "config", "user.email", "test@test.com"]);
    run(&dir, &["git", "config", "user.name", "Test"]);
    std::fs::write(dir.join("file.txt"), "hello").unwrap();
    run(&dir, &["git", "add", "."]);
    run(&dir, &["git", "commit", "-m", "init"]);

    // Modify and stage
    std::fs::write(dir.join("file.txt"), "staged").unwrap();
    run(&dir, &["git", "add", "."]);

    let unstaged = Command::new("git")
        .args(["diff", "--quiet"])
        .current_dir(&dir)
        .status()
        .unwrap();
    assert!(unstaged.success(), "should have no unstaged changes");

    let staged = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(&dir)
        .status()
        .unwrap();
    assert!(!staged.success(), "should detect staged changes");
}

#[test]
fn test_git_log_format() {
    let dir = tempdir();
    run(&dir, &["git", "init"]);
    run(&dir, &["git", "config", "user.email", "test@test.com"]);
    run(&dir, &["git", "config", "user.name", "Test"]);
    std::fs::write(dir.join("file.txt"), "hello").unwrap();
    run(&dir, &["git", "add", "."]);
    run(&dir, &["git", "commit", "-m", "test commit message"]);

    let output = Command::new("git")
        .args(["log", "-1", "--format=%s"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let summary = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(summary, "test commit message");

    let output = Command::new("git")
        .args(["log", "-1", "--format=%ct"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let ts: i64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap();
    assert!(ts > 0);
}

#[test]
fn test_sync_state_no_upstream() {
    let dir = tempdir();
    run(&dir, &["git", "init"]);
    run(&dir, &["git", "config", "user.email", "test@test.com"]);
    run(&dir, &["git", "config", "user.name", "Test"]);
    std::fs::write(dir.join("file.txt"), "hello").unwrap();
    run(&dir, &["git", "add", "."]);
    run(&dir, &["git", "commit", "-m", "init"]);

    // No upstream — rev-parse @{u} should fail
    let status = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .current_dir(&dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success(), "fresh repo should not have upstream");
}

#[test]
fn test_sync_state_ahead_behind() {
    // Create a "remote" bare repo and two clones that diverge
    let upstream = tempdir();
    run(&upstream, &["git", "init", "--bare"]);

    let dir = tempdir();
    run(&dir, &["git", "init"]);
    run(&dir, &["git", "config", "user.email", "test@test.com"]);
    run(&dir, &["git", "config", "user.name", "Test"]);
    run(&dir, &["git", "remote", "add", "origin", upstream.to_str().unwrap()]);
    std::fs::write(dir.join("file.txt"), "v1").unwrap();
    run(&dir, &["git", "add", "."]);
    run(&dir, &["git", "commit", "-m", "v1"]);
    // Determine current branch (could be main or master depending on git config)
    let branch_out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let branch = String::from_utf8_lossy(&branch_out.stdout).trim().to_string();
    run(&dir, &["git", "push", "-u", "origin", &branch]);

    // Add 2 local commits → ahead by 2
    std::fs::write(dir.join("file.txt"), "v2").unwrap();
    run(&dir, &["git", "commit", "-am", "v2"]);
    std::fs::write(dir.join("file.txt"), "v3").unwrap();
    run(&dir, &["git", "commit", "-am", "v3"]);

    let output = Command::new("git")
        .args(["rev-list", "--left-right", "--count", "@{u}...HEAD"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = s.split_whitespace().collect();
    assert_eq!(parts.len(), 2);
    let behind: u32 = parts[0].parse().unwrap();
    let ahead: u32 = parts[1].parse().unwrap();
    assert_eq!(behind, 0);
    assert_eq!(ahead, 2);
}

// Helpers

fn format_age(timestamp: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
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

fn tempdir() -> std::path::PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("git-watch-test-{}-{}", std::process::id(), id));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, args: &[&str]) {
    let status = Command::new(args[0])
        .args(&args[1..])
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "command failed: {:?}", args);
}
