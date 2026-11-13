use anyhow::{Context, Result};
use base64::Engine;
use sha1::{Digest, Sha1};
use reqwest::blocking::{Client, Response};
use reqwest::header::ACCEPT;
use serde::Deserialize;
use std::{collections::HashSet, io::Read, time::Duration};

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

#[derive(Deserialize)]
struct BlobResponse {
    content: String,
    encoding: String,
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
    if let Some(seconds) = retry_after.and_then(|value| value.trim().parse::<u64>().ok()) {
        return Duration::from_secs(seconds.min(MAX_RETRY_AFTER_SECS));
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

fn api_get(url: &str, token: &str) -> Result<Response> {
    let client = Client::builder()
        .user_agent("mtg-asset-auditor")
        .timeout(Duration::from_secs(60))
        .build()?;

    for attempt in 0..=MAX_HTTP_RETRIES {
        let mut request = client.get(url).header(ACCEPT, "application/vnd.github+json");
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
const MAX_BLOB_RESPONSE_BYTES: u64 = 128 * 1024 * 1024;

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

fn validate_decoded_blob_size(actual: usize, expected: u64) -> Result<()> {
    if actual as u64 != expected {
        anyhow::bail!(
            "Taille du blob incohérente : {} octets reçus, {} attendus.",
            actual,
            expected
        );
    }
    Ok(())
}

pub fn verify_git_blob_sha(expected_sha: &str, bytes: &[u8]) -> Result<()> {
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", bytes.len()).as_bytes());
    hasher.update(bytes);
    let actual = format!("{:x}", hasher.finalize());
    if !actual.eq_ignore_ascii_case(expected_sha.trim()) {
        anyhow::bail!("Empreinte Git blob invalide : {} reçue, {} attendue.", actual, expected_sha);
    }
    Ok(())
}

pub fn download_blob(
    owner: &str,
    repository: &str,
    sha: &str,
    token: &str,
    expected_size: Option<u64>,
) -> Result<Vec<u8>> {
    let expected_size = validate_blob_size(expected_size)?;
    let url = format!(
        "{}/git/blobs/{}",
        repository_base(owner, repository),
        urlencoding::encode(sha.trim())
    );
    let response = api_get(&url, token)?;
    if response.content_length().is_some_and(|length| length > MAX_BLOB_RESPONSE_BYTES) {
        anyhow::bail!("Réponse blob GitHub supérieure à la limite de sécurité.");
    }
    let mut limited = response.take(MAX_BLOB_RESPONSE_BYTES + 1);
    let mut body = Vec::new();
    limited.read_to_end(&mut body).context("Lecture de la réponse blob impossible")?;
    if body.len() as u64 > MAX_BLOB_RESPONSE_BYTES {
        anyhow::bail!("Réponse blob GitHub tronquée : taille supérieure à la limite de sécurité.");
    }
    let blob: BlobResponse = serde_json::from_slice(&body).context("Réponse blob GitHub invalide")?;
    if blob.encoding != "base64" {
        anyhow::bail!("Encodage de blob non pris en charge : {}", blob.encoding);
    }
    let compact: String = blob.content.chars().filter(|character| !character.is_ascii_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(compact)
        .context("Décodage base64 impossible")?;
    validate_decoded_blob_size(bytes.len(), expected_size)?;
    verify_git_blob_sha(sha, &bytes)?;
    Ok(bytes)
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
    fn rejects_decoded_blob_size_mismatches() {
        assert!(validate_decoded_blob_size(4, 5).is_err());
        assert!(validate_decoded_blob_size(5, 5).is_ok());
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
