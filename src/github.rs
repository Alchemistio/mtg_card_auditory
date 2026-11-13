use anyhow::{Context, Result};
use base64::Engine;
use reqwest::blocking::{Client, Response};
use reqwest::header::ACCEPT;
use serde::Deserialize;
use std::{collections::HashSet, time::Duration};

use crate::models::RemoteEntry;

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
    let trimmed = input.trim().trim_end_matches('/');
    let without_scheme = trimmed.strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    let without_host = without_scheme.strip_prefix("github.com/")
        .context("URL GitHub attendue (github.com/owner/repository)")?;
    let mut parts = without_host.split('/');
    let owner = parts.next().unwrap_or_default();
    let repository = parts.next().unwrap_or_default().trim_end_matches(".git");
    if owner.is_empty() || repository.is_empty() || owner.contains(':') || owner.contains('@') {
        anyhow::bail!("URL de dépôt GitHub invalide.");
    }
    Ok((owner.to_string(), repository.to_string()))
}

fn api_get(url: &str, token: &str) -> Result<Response> {
    let client = Client::builder()
        .user_agent("mtg-asset-auditor")
        .timeout(Duration::from_secs(60))
        .build()?;
    let mut request = client.get(url).header(ACCEPT, "application/vnd.github+json");
    if !token.trim().is_empty() {
        request = request.bearer_auth(token.trim());
    }
    let response = request.send().with_context(|| format!("Requête GitHub impossible : {}", url))?;
    let status = response.status();
    if !status.is_success() {
        let remaining = response.headers().get("x-ratelimit-remaining").and_then(|value| value.to_str().ok()).unwrap_or("").to_string();
        let reset = response.headers().get("x-ratelimit-reset").and_then(|value| value.to_str().ok()).unwrap_or("").to_string();
        let retry_after = response.headers().get("retry-after").and_then(|value| value.to_str().ok()).unwrap_or("").to_string();
        let body = response.text().unwrap_or_default();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS || (status == reqwest::StatusCode::FORBIDDEN && remaining == "0") {
            anyhow::bail!("Limite de l'API GitHub atteinte. Retry-After={} s ; reset Unix={} ; détail={}", retry_after, reset, body);
        }
        anyhow::bail!("GitHub a répondu {}: {}", status, body);
    }
    Ok(response)
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

pub fn list_repository_tree(owner: &str, repository: &str, branch: &str, token: &str) -> Result<Vec<RemoteEntry>> {
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
        return Ok(recursive.tree);
    }

    let mut walker = TreeWalker {
        owner,
        repository,
        token,
        active_trees: HashSet::new(),
        output: Vec::new(),
    };
    walker.collect(&tree_sha, "", 0)?;
    Ok(walker.output)
}

pub fn download_blob(owner: &str, repository: &str, sha: &str, token: &str) -> Result<Vec<u8>> {
    let url = format!(
        "{}/git/blobs/{}",
        repository_base(owner, repository),
        urlencoding::encode(sha.trim())
    );
    let blob: BlobResponse = api_get(&url, token)?.json().context("Réponse blob GitHub invalide")?;
    if blob.encoding != "base64" {
        anyhow::bail!("Encodage de blob non pris en charge : {}", blob.encoding);
    }
    let compact = blob.content.replace('\n', "");
    base64::engine::general_purpose::STANDARD
        .decode(compact)
        .context("Décodage base64 impossible")
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
