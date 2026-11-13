use anyhow::{Context, Result};
use sha1::{Digest, Sha1};
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, HeaderValue};
use serde::Deserialize;
use std::{collections::HashSet, io::{Read, Write}, path::Path, time::{Duration, SystemTime}};

use crate::models::{RemoteEntry, RepositoryTree};

#[derive(Deserialize)]
struct TreeResponse {
    tree: Vec<RemoteEntry>,
    truncated: bool,
}

#[derive(Deserialize)]
struct RepositoryResponse {
    default_branch: String,
}

#[derive(Deserialize)]
struct CommitResponse {
    sha: String,
    commit: CommitDetails,
}

#[derive(Deserialize)]
struct CommitDetails {
    tree: TreeSha,
}

#[derive(Deserialize)]
struct TreeSha {
    sha: String,
}

pub fn parse_repository_url(input: &str) -> Result<(String, String)> {
    let input = input.trim();
    if input.is_empty() {
        anyhow::bail!("URL de dépôt GitHub vide.");
    }
    let candidate = if input.contains("://") {
        input.to_string()
    } else {
        format!("https://{}", input)
    };
    let url = url::Url::parse(&candidate).context("URL GitHub invalide.")?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        anyhow::bail!("Une URL HTTPS canonique de github.com est attendue.");
    }
    let parts: Vec<_> = url.path_segments()
        .context("Chemin de dépôt GitHub invalide.")?
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() != 2 {
        anyhow::bail!("Indique exactement github.com/owner/repository, sans sous-chemin.");
    }
    let owner = parts[0];
    let repository = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
    let valid_owner = !owner.is_empty()
        && owner.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    let valid_repository = !repository.is_empty()
        && repository != "."
        && repository != ".."
        && repository.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    if !valid_owner || !valid_repository {
        anyhow::bail!("Nom de propriétaire ou de dépôt GitHub invalide.");
    }
    Ok((owner.to_string(), repository.to_string()))
}

const MAX_HTTP_RETRIES: usize = 3;
const MAX_RETRY_AFTER_SECS: u64 = 10;

fn retry_delay(attempt: usize, retry_after: Option<&str>) -> Duration {
    if let Some(value) = retry_after {
        if let Ok(seconds) = value.trim().parse::<u64>() {
            return Duration::from_secs(seconds.min(MAX_RETRY_AFTER_SECS));
        }
        if let Ok(deadline) = httpdate::parse_http_date(value) {
            let delay = deadline.duration_since(SystemTime::now()).unwrap_or_default();
            return delay.min(Duration::from_secs(MAX_RETRY_AFTER_SECS));
        }
    }
    Duration::from_millis(500_u64.saturating_mul(1_u64 << attempt.min(4)))
}

fn retryable_status(status: reqwest::StatusCode, retry_after_present: bool) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status == reqwest::StatusCode::BAD_GATEWAY
        || status == reqwest::StatusCode::SERVICE_UNAVAILABLE
        || status == reqwest::StatusCode::GATEWAY_TIMEOUT
        || (status == reqwest::StatusCode::FORBIDDEN && retry_after_present)
}

fn api_get_with_accept(url: &str, token: &str, accept: &str) -> Result<Response> {
    let client = Client::builder()
        .user_agent("mtg-asset-auditor")
        .timeout(Duration::from_secs(60))
        .build()?;

    for attempt in 0..=MAX_HTTP_RETRIES {
        let mut request = client.get(url).header(ACCEPT, HeaderValue::from_str(accept)?);
        if !token.trim().is_empty() {
            request = request.bearer_auth(token.trim());
        }

        let response = match request.send() {
            Ok(response) => response,
            Err(error) if attempt < MAX_HTTP_RETRIES && (error.is_timeout() || error.is_connect()) => {
                std::thread::sleep(retry_delay(attempt, None));
                continue;
            }
            Err(error) => {
                return Err(error).with_context(|| format!("Requête GitHub impossible : {}", url));
            }
        };

        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        let remaining = response.headers().get("x-ratelimit-remaining")
            .and_then(|value| value.to_str().ok()).unwrap_or("").to_string();
        let reset = response.headers().get("x-ratelimit-reset")
            .and_then(|value| value.to_str().ok()).unwrap_or("").to_string();
        let retry_after = response.headers().get("retry-after")
            .and_then(|value| value.to_str().ok()).map(str::to_owned);
        let retryable = retryable_status(status, retry_after.is_some());
        let body = response.text().unwrap_or_default();

        if retryable && attempt < MAX_HTTP_RETRIES {
            std::thread::sleep(retry_delay(attempt, retry_after.as_deref()));
            continue;
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS
            || (status == reqwest::StatusCode::FORBIDDEN && remaining == "0")
            || (status == reqwest::StatusCode::FORBIDDEN && retry_after.is_some())
        {
            anyhow::bail!(
                "Limite de l'API GitHub atteinte après {} tentative(s). Retry-After={} s ; quota restant={} ; reset Unix={} ; détail={}",
                attempt + 1,
                retry_after.as_deref().unwrap_or("non communiqué"),
                if remaining.is_empty() { "inconnu" } else { &remaining },
                if reset.is_empty() { "inconnu" } else { &reset },
                body
            );
        }
        anyhow::bail!("GitHub a répondu {}: {}", status, body);
    }

    anyhow::bail!("Échec inattendu des tentatives HTTP GitHub.")
}

fn api_get(url: &str, token: &str) -> Result<Response> {
    api_get_with_accept(url, token, "application/vnd.github+json")
}

fn repository_base(owner: &str, repository: &str) -> String {
    format!(
        "https://api.github.com/repos/{}/{}",
        urlencoding::encode(owner.trim()),
        urlencoding::encode(repository.trim())
    )
}

fn fetch_tree(owner: &str, repository: &str, tree_sha: &str, recursive: bool, token: &str) -> Result<TreeResponse> {
    let suffix = if recursive { "?recursive=1" } else { "" };
    let url = format!("{}/git/trees/{}{}", repository_base(owner, repository), tree_sha, suffix);
    api_get(&url, token)?.json().context("Réponse arbre GitHub invalide")
}

struct TreeWalker<'a> {
    owner: &'a str,
    repository: &'a str,
    token: &'a str,
    active_trees: HashSet<String>,
    output: Vec<RemoteEntry>,
}

impl TreeWalker<'_> {
    fn collect(&mut self, tree_sha: &str, prefix: &str, depth: usize) -> Result<()> {
        if depth > 128 {
            anyhow::bail!("Profondeur d'arbre GitHub supérieure à la limite de sécurité (128).");
        }
        if !self.active_trees.insert(tree_sha.to_string()) {
            anyhow::bail!("Cycle détecté dans le graphe des arbres GitHub.");
        }
        let response = fetch_tree(self.owner, self.repository, tree_sha, false, self.token)?;
        if response.truncated {
            anyhow::bail!("Un sous-arbre GitHub non récursif est lui-même tronqué ; inventaire incomplet refusé.");
        }
        for mut entry in response.tree {
            let full_path = if prefix.is_empty() {
                entry.path.clone()
            } else {
                format!("{}/{}", prefix, entry.path)
            };
            if entry.kind == "tree" {
                let child_sha = entry.sha.clone();
                entry.path = full_path.clone();
                self.output.push(entry);
                self.collect(&child_sha, &full_path, depth + 1)?;
            } else {
                entry.path = full_path;
                self.output.push(entry);
            }
        }
        self.active_trees.remove(tree_sha);
        Ok(())
    }
}

pub fn list_repository_tree(owner: &str, repository: &str, branch: &str, token: &str) -> Result<RepositoryTree> {
    let owner = owner.trim();
    let repository = repository.trim();
    if owner.is_empty() || repository.is_empty() {
        anyhow::bail!("Le propriétaire et le nom du dépôt sont obligatoires.");
    }
    let repo_base = repository_base(owner, repository);
    let reference = if branch.trim().is_empty() {
        let metadata: RepositoryResponse = api_get(&repo_base, token)?
            .json()
            .context("Impossible de déterminer la branche par défaut")?;
        metadata.default_branch
    } else {
        branch.trim().to_string()
    };
    let commit_url = format!("{}/commits/{}", repo_base, urlencoding::encode(&reference));
    let commit: CommitResponse = api_get(&commit_url, token)?
        .json()
        .context("Impossible de résoudre la référence en commit immuable")?;
    let tree_sha = commit.commit.tree.sha;
    let recursive = fetch_tree(owner, repository, &tree_sha, true, token)?;
    if !recursive.truncated {
        return Ok(RepositoryTree {
            commit_sha: commit.sha,
            tree_sha,
            entries: recursive.tree,
        });
    }

    let mut walker = TreeWalker {
        owner,
        repository,
        token,
        active_trees: HashSet::new(),
        output: Vec::new(),
    };
    walker.collect(&tree_sha, "", 0)?;
    Ok(RepositoryTree {
        commit_sha: commit.sha,
        tree_sha,
        entries: walker.output,
    })
}

const MAX_BLOB_BYTES: u64 = 90 * 1024 * 1024;

fn validate_blob_size(expected_size: Option<u64>) -> Result<u64> {
    let size = expected_size.context("Taille du blob absente dans l'arbre GitHub ; téléchargement refusé.")?;
    if size > MAX_BLOB_BYTES {
        anyhow::bail!(
            "Blob trop volumineux : {} octets, limite actuelle {} octets.",
            size,
            MAX_BLOB_BYTES
        );
    }
    Ok(size)
}


#[cfg(test)]
fn verify_git_blob_sha(expected_sha: &str, bytes: &[u8]) -> Result<()> {
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", bytes.len()).as_bytes());
    hasher.update(bytes);
    let actual = format!("{:x}", hasher.finalize());
    if !actual.eq_ignore_ascii_case(expected_sha.trim()) {
        anyhow::bail!("Empreinte Git blob invalide : {} reçue, {} attendue.", actual, expected_sha);
    }
    Ok(())
}

pub fn verify_git_blob_reader<R: Read>(expected_sha: &str, reader: &mut R, expected_size: u64) -> Result<()> {
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", expected_size).as_bytes());
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 { break; }
        total = total.checked_add(count as u64).context("Taille du blob débordée")?;
        if total > expected_size { anyhow::bail!("Fichier plus grand que la taille attendue."); }
        hasher.update(&buffer[..count]);
    }
    if total != expected_size { anyhow::bail!("Fichier incomplet : {} octets, {} attendus.", total, expected_size); }
    let actual = format!("{:x}", hasher.finalize());
    if !actual.eq_ignore_ascii_case(expected_sha.trim()) {
        anyhow::bail!("Empreinte Git blob invalide : {} reçue, {} attendue.", actual, expected_sha);
    }
    Ok(())
}

#[cfg(test)]
pub fn verify_git_blob_file(expected_sha: &str, path: &Path, expected_size: u64) -> Result<()> {
    let mut file = std::fs::File::open(path)?;
    verify_git_blob_reader(expected_sha, &mut file, expected_size)
}

pub fn download_blob_to_file(
    owner: &str,
    repository: &str,
    sha: &str,
    token: &str,
    expected_size: Option<u64>,
    workspace: &cap_std::fs::Dir,
    destination: &Path,
) -> Result<()> {
    let expected_size = validate_blob_size(expected_size)?;
    if destination.is_absolute()
        || destination.components().any(|component| !matches!(component, std::path::Component::Normal(_))) {
        anyhow::bail!("Chemin temporaire de cache non relatif ou invalide.");
    }
    let name = destination.file_name().context("Nom du fichier temporaire absent")?;
    let parent = destination.parent().unwrap_or_else(|| Path::new(""));
    let parent_dir = if parent.as_os_str().is_empty() {
        workspace.try_clone()?
    } else {
        workspace.open_dir(parent)?
    };
    let url = format!("{}/git/blobs/{}", repository_base(owner, repository), urlencoding::encode(sha.trim()));
    let mut response = api_get_with_accept(&url, token, "application/vnd.github.raw+json")?;
    if response.content_length().is_some_and(|length| length > expected_size) {
        anyhow::bail!("Réponse blob GitHub supérieure à la taille déclarée.");
    }
    let mut options = cap_std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = parent_dir.open_with(name, &options)
        .with_context(|| format!("Fichier temporaire déjà présent ou inaccessible : {}", destination.display()))?;
    let result = (|| -> Result<()> {
        let mut hasher = Sha1::new();
        hasher.update(format!("blob {}\\0", expected_size).as_bytes());
        let mut buffer = [0u8; 64 * 1024];
        let mut total = 0u64;
        loop {
            let count = response.read(&mut buffer).context("Lecture du flux blob GitHub impossible")?;
            if count == 0 { break; }
            total = total.checked_add(count as u64).context("Taille du blob débordée")?;
            if total > expected_size { anyhow::bail!("Blob GitHub plus grand que la taille déclarée."); }
            hasher.update(&buffer[..count]);
            file.write_all(&buffer[..count])?;
        }
        if total != expected_size { anyhow::bail!("Blob GitHub incomplet : {} octets reçus, {} attendus.", total, expected_size); }
        let actual = format!("{:x}", hasher.finalize());
        if !actual.eq_ignore_ascii_case(sha.trim()) {
            anyhow::bail!("Empreinte Git blob invalide : {} reçue, {} attendue.", actual, sha);
        }
        file.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        drop(file);
        let _ = parent_dir.remove_file(name);
    }
    result
}

#[cfg(test)]
mod blob_size_tests {
    use super::*;

    #[test]
    fn verifies_git_blob_sha1_with_git_object_header() {
        let bytes = b"hello world\n";
        assert!(verify_git_blob_sha("3b18e512dba79e4c8300dd08aeb37f8e728b8dad", bytes).is_ok());
        assert!(verify_git_blob_sha("3b18e512dba79e4c8300dd08aeb37f8e728b8dad", b"tampered").is_err());
    }

    #[test]
    fn verifies_git_blob_file_incrementally() {
        let path = std::env::temp_dir().join(format!("mtg-auditor-blob-test-{}.tmp", std::process::id()));
        let bytes = b"hello world\n";
        std::fs::write(&path, bytes).unwrap();
        assert!(verify_git_blob_file("3b18e512dba79e4c8300dd08aeb37f8e728b8dad", &path, bytes.len() as u64).is_ok());
        assert!(verify_git_blob_file("3b18e512dba79e4c8300dd08aeb37f8e728b8dad", &path, 1).is_err());
        assert!(verify_git_blob_file("0000000000000000000000000000000000000000", &path, bytes.len() as u64).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_unknown_and_oversized_blob_lengths() {
        assert!(validate_blob_size(None).is_err());
        assert!(validate_blob_size(Some(MAX_BLOB_BYTES + 1)).is_err());
        assert_eq!(validate_blob_size(Some(0)).unwrap(), 0);
        assert_eq!(validate_blob_size(Some(MAX_BLOB_BYTES)).unwrap(), MAX_BLOB_BYTES);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_transient_github_responses_only() {
        assert!(retryable_status(reqwest::StatusCode::TOO_MANY_REQUESTS, false));
        assert!(retryable_status(reqwest::StatusCode::BAD_GATEWAY, false));
        assert!(retryable_status(reqwest::StatusCode::SERVICE_UNAVAILABLE, false));
        assert!(retryable_status(reqwest::StatusCode::FORBIDDEN, true));
        assert!(!retryable_status(reqwest::StatusCode::FORBIDDEN, false));
        assert!(!retryable_status(reqwest::StatusCode::NOT_FOUND, true));
    }

    #[test]
    fn retry_delay_is_bounded_and_honors_retry_after_seconds() {
        assert_eq!(retry_delay(0, None), Duration::from_millis(500));
        assert_eq!(retry_delay(2, None), Duration::from_millis(2000));
        assert_eq!(retry_delay(0, Some("4")), Duration::from_secs(4));
        assert_eq!(retry_delay(0, Some("999")), Duration::from_secs(MAX_RETRY_AFTER_SECS));
        assert_eq!(retry_delay(0, Some("invalid")), Duration::from_millis(500));
        assert_eq!(
            retry_delay(0, Some("Wed, 21 Oct 2015 07:28:00 GMT")),
            Duration::ZERO
        );
        let future_date = httpdate::fmt_http_date(
            SystemTime::now() + Duration::from_secs(3600)
        );
        assert_eq!(
            retry_delay(0, Some(&future_date)),
            Duration::from_secs(MAX_RETRY_AFTER_SECS)
        );
    }

    #[test]
    fn parses_supported_github_repository_urls() {
        assert_eq!(parse_repository_url("https://github.com/octocat/Hello-World").unwrap(),
            ("octocat".to_string(), "Hello-World".to_string()));
        assert_eq!(parse_repository_url("https://github.com/octocat/Hello-World.git/").unwrap(),
            ("octocat".to_string(), "Hello-World".to_string()));
    }

    #[test]
    fn rejects_non_github_urls() {
        assert!(parse_repository_url("https://example.com/octocat/repo").is_err());
        assert!(parse_repository_url("https://github.com/octocat").is_err());
        assert!(parse_repository_url("https://github.com.evil.test/octocat/repo").is_err());
        assert!(parse_repository_url("http://github.com/octocat/repo").is_err());
        assert!(parse_repository_url("https://github.com/octocat/repo/tree/main").is_err());
        assert!(parse_repository_url("https://github.com/octocat/repo?tab=readme").is_err());
    }
}
