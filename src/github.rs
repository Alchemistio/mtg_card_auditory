use anyhow::{Context, Result};
use base64::Engine;
use reqwest::blocking::{Client, Response};
use reqwest::header::ACCEPT;
use serde::Deserialize;

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
    let client = Client::builder().user_agent("mtg-asset-auditor").build()?;
    let mut request = client.get(url).header(ACCEPT, "application/vnd.github+json");
    if !token.trim().is_empty() {
        request = request.bearer_auth(token.trim());
    }
    let response = request.send().with_context(|| format!("Requête GitHub impossible : {}", url))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        anyhow::bail!("GitHub a répondu {}: {}", status, body);
    }
    Ok(response)
}

pub fn list_repository_tree(owner: &str, repository: &str, branch: &str, token: &str) -> Result<Vec<RemoteEntry>> {
    let owner = owner.trim();
    let repository = repository.trim();
    if owner.is_empty() || repository.is_empty() {
        anyhow::bail!("Le propriétaire et le nom du dépôt sont obligatoires.");
    }
    let repo_base = format!(
        "https://api.github.com/repos/{}/{}",
        urlencoding::encode(owner),
        urlencoding::encode(repository)
    );
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
    let tree_url = format!("{}/git/trees/{}?recursive=1", repo_base, commit.sha);
    let tree: TreeResponse = api_get(&tree_url, token)?.json().context("Réponse GitHub invalide")?;
    if tree.truncated {
        anyhow::bail!("Arbre GitHub tronqué. L'exploration récursive par sous-arbres sera ajoutée dans la phase de robustesse.");
    }
    Ok(tree.tree)
}

pub fn download_blob(owner: &str, repository: &str, sha: &str, token: &str) -> Result<Vec<u8>> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/git/blobs/{}",
        urlencoding::encode(owner.trim()),
        urlencoding::encode(repository.trim()),
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
