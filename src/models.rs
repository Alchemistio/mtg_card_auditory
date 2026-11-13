use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub sha: String,
    pub size: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct RepositoryTree {
    pub commit_sha: String,
    pub tree_sha: String,
    pub entries: Vec<RemoteEntry>,
}

#[derive(Debug, Serialize)]
pub struct DownloadManifest {
    pub schema_version: u32,
    pub provider: String,
    pub owner: String,
    pub repository: String,
    pub requested_reference: String,
    pub commit_sha: String,
    pub tree_sha: String,
    pub downloaded_files: Vec<DownloadedFile>,
}

#[derive(Debug, Serialize)]
pub struct DownloadedFile {
    pub path: String,
    pub blob_sha: String,
    pub size_bytes: u64,
}
