use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum StatusKind {
    Clean,
    StagedOnly,
    UnstagedOnly,
    Both,
    Error,
}

#[derive(Debug, Clone, Default)]
pub struct SyncState {
    pub has_upstream: bool,
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Debug, Clone)]
pub struct RepoState {
    pub name: String,
    pub path: PathBuf,
    pub status: StatusKind,
    pub commit_summary: String,
    pub commit_timestamp: Option<i64>,
    pub error_msg: Option<String>,
    pub sync: SyncState,
}

impl RepoState {
    pub fn new_error(name: String, path: PathBuf, msg: String) -> Self {
        Self {
            name,
            path,
            status: StatusKind::Error,
            commit_summary: String::new(),
            commit_timestamp: None,
            error_msg: Some(msg),
            sync: SyncState::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Push,
    Pull,
}

pub enum Message {
    RepoChanged(PathBuf),
    ReconcileAll,
    FetchStarted,
    FetchFinished,
    OpFinished {
        repo: PathBuf,
        kind: OpKind,
        result: Result<(), String>,
    },
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
