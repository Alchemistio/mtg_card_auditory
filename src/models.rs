use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub sha: String,
    pub size: Option<u64>,
}
