use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Clone)]
pub enum StatusKind {
    Clean,
    StagedOnly,
    UnstagedOnly,
    Both,
    Error,
}

#[derive(Debug, Clone)]
pub struct RepoState {
    pub name: String,
    pub path: PathBuf,
    pub status: StatusKind,
    pub commit_summary: String,
    pub commit_timestamp: Option<i64>,
    pub last_refresh: Instant,
    pub error_msg: Option<String>,
}

impl RepoState {
    pub fn new_error(name: String, path: PathBuf, msg: String) -> Self {
        Self {
            name,
            path,
            status: StatusKind::Error,
            commit_summary: String::new(),
            commit_timestamp: None,
            last_refresh: Instant::now(),
            error_msg: Some(msg),
        }
    }
}

pub enum Message {
    RepoChanged(PathBuf),
    ReconcileAll,
    Quit,
}

#[derive(Debug, Clone)]
pub struct DetailedStatus {
    pub staged: Vec<String>,
    pub unstaged: Vec<String>,
    pub untracked: Vec<String>,
}

pub enum ViewMode {
    List,
    Detail { index: usize, status: DetailedStatus },
}
