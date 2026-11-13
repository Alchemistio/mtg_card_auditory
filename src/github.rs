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
struct BlobResponse {
    content: String,
    encoding: String,
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
    let reference = if branch.trim().is_empty() {
        let repo_url = format!(
            "https://api.github.com/repos/{}/{}",
            urlencoding::encode(owner),
            urlencoding::encode(repository)
        );
        let metadata: RepositoryResponse = api_get(&repo_url, token)?
            .json()
            .context("Impossible de déterminer la branche par défaut")?;
        metadata.default_branch
    } else {
        branch.trim().to_string()
    };
    let url = format!(
        "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
        urlencoding::encode(owner),
        urlencoding::encode(repository),
        urlencoding::encode(&reference)
    );
    let tree: TreeResponse = api_get(&url, token)?.json().context("Réponse GitHub invalide")?;
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
    Ok(base64::engine::general_purpose::STANDARD.decode(compact).context("Décodage base64 impossible")?)
}
