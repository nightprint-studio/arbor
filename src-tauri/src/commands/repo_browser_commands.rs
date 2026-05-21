use crate::error::Result;
use crate::git_provider::repo_impl::{
    RemoteAccount, RemoteRepo, RemoteTreeEntry, RemoteFileContent,
    list_accounts, list_repos, browse_tree, get_file_content, download_file,
};

/// List all connected remote accounts (GitHub + GitLab).
#[tauri::command]
pub async fn rb_list_accounts() -> Vec<RemoteAccount> {
    list_accounts().await
}

/// Fetch all repositories accessible to the given provider account.
/// `provider`: "github" | "gitlab"
#[tauri::command]
pub async fn rb_list_repos(provider: String) -> Result<Vec<RemoteRepo>> {
    list_repos(&provider).await
}

/// List files and directories at `path` within a remote repository.
/// `path`: relative path inside the repo ("" for root).
/// `branch`: branch/tag/sha to browse.
#[tauri::command]
pub async fn rb_browse_tree(
    provider:   String,
    full_name:  String,
    path:       String,
    branch:     String,
) -> Result<Vec<RemoteTreeEntry>> {
    browse_tree(&provider, &full_name, &path, &branch).await
}

/// Fetch the content of a single file for inline preview.
#[tauri::command]
pub async fn rb_get_file_content(
    provider:  String,
    full_name: String,
    path:      String,
    branch:    String,
) -> Result<RemoteFileContent> {
    get_file_content(&provider, &full_name, &path, &branch).await
}

/// Download a remote file to a local path.
#[tauri::command]
pub async fn rb_download_file(
    provider:  String,
    full_name: String,
    path:      String,
    branch:    String,
    dest_path: String,
) -> Result<()> {
    download_file(&provider, &full_name, &path, &branch, &dest_path).await
}
