use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};

// ── Remote repository payloads ───────────────────────────────────────────────
// `RemoteRepo` is the canonical "remote repository" struct; the trait surface
// speaks the spec name `RemoteRepoInfo` (alias below) while serde stays
// byte-identical to the current frontend types.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAccount {
    pub provider:      String,         // "github" | "gitlab"
    pub username:      String,
    pub display_name:  Option<String>,
    pub avatar_url:    Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteRepo {
    pub id:               String,      // numeric ID string from API
    pub name:             String,
    pub namespace:        String,      // org or user login
    pub full_name:        String,      // "namespace/name"
    pub description:      Option<String>,
    pub private:          bool,
    pub default_branch:   String,
    pub language:         Option<String>,
    pub stars:            u32,
    pub updated_at:       String,      // ISO 8601
    pub clone_url_https:  String,
    pub clone_url_ssh:    Option<String>,
    pub web_url:          String,
    pub provider:         String,
    pub is_fork:          bool,
    pub is_archived:      bool,
    pub size_kb:          Option<u64>,
}

/// Spec name used by the `GitProvider` trait surface.
pub type RemoteRepoInfo = RemoteRepo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteTreeEntry {
    pub name:       String,
    pub path:       String,
    pub entry_type: String,    // "file" | "dir" | "submodule" | "symlink"
    pub size:       Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFileContent {
    pub path:       String,
    pub content:    String,            // UTF-8 text (empty for binary/image)
    pub image_data: Option<String>,    // data:<mime>;base64,<data>
    pub size:       u64,
    pub is_binary:  bool,
    pub is_image:   bool,
    pub mime_type:  Option<String>,
}

// ── Trait-vocabulary request / filter types ──────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepoVisibility {
    Public,
    Private,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoCreateRequest {
    pub name:        String,
    pub description: Option<String>,
    pub visibility:  RepoVisibility,
    /// GitHub-only: organization to create the repo under (None → user account).
    pub org:         Option<String>,
    /// GitLab-only: numeric namespace ID (None → user namespace).
    pub namespace_id: Option<u64>,
}

/// Pagination + filter knobs for `list_user_repos` / `list_org_repos`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListReposOpts {
    pub page:     Option<u32>,
    pub per_page: Option<u32>,
    /// Free-text query (provider-specific behavior).
    pub query:    Option<String>,
}

/// Lightweight repo identifier passed to repo-scoped trait methods.
///
/// GitHub: `owner_or_path` = owner login, `name` = repo name.
/// GitLab: `owner_or_path` = full project path (e.g. `myorg/sub/myrepo`),
/// `name` = `None` (the path is self-contained).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRef {
    pub owner_or_path: String,
    pub name:          Option<String>,
}

impl RepoRef {
    pub fn github(owner: impl Into<String>, name: impl Into<String>) -> Self {
        Self { owner_or_path: owner.into(), name: Some(name.into()) }
    }

    pub fn gitlab(project_path: impl Into<String>) -> Self {
        Self { owner_or_path: project_path.into(), name: None }
    }
}

// ── Pure file-content helpers (shared by the github/gitlab impls) ─────────────

/// Max bytes of a text file inlined as a preview; larger files are flagged
/// binary (`content` stays empty).
pub const MAX_PREVIEW_BYTES: u64 = 512 * 1024; // 512 KB
/// Max bytes of an image inlined as a base64 `data:` URL; larger images report
/// `is_image = true` but carry no `image_data`.
pub const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024; // 5 MB

/// Classify raw remote-file bytes into a `RemoteFileContent`: small images are
/// inlined as a `data:<mime>;base64,…` URL, small UTF-8 text as `content`, and
/// everything else (oversize, non-UTF-8) is flagged binary. Pure — every
/// provider's `get_file_content` funnels its fetched bytes through this so the
/// preview/binary semantics stay identical across hosts.
pub fn build_file_content(path: &str, bytes: Vec<u8>, mime: &str) -> RemoteFileContent {
    let size     = bytes.len() as u64;
    let is_image = mime.starts_with("image/");

    if is_image {
        if size > MAX_IMAGE_BYTES {
            return RemoteFileContent {
                path: path.into(), content: String::new(), image_data: None,
                size, is_binary: true, is_image: true,
                mime_type: Some(mime.into()),
            };
        }
        return RemoteFileContent {
            path:       path.into(),
            content:    String::new(),
            image_data: Some(format!("data:{mime};base64,{}", BASE64.encode(&bytes))),
            size,
            is_binary:  false,
            is_image:   true,
            mime_type:  Some(mime.into()),
        };
    }

    if size > MAX_PREVIEW_BYTES {
        return RemoteFileContent {
            path: path.into(), content: String::new(), image_data: None,
            size, is_binary: true, is_image: false,
            mime_type: Some(mime.into()),
        };
    }

    match String::from_utf8(bytes) {
        Ok(text) => RemoteFileContent {
            path: path.into(), content: text, image_data: None,
            size, is_binary: false, is_image: false,
            mime_type: Some(mime.into()),
        },
        Err(_) => RemoteFileContent {
            path: path.into(), content: String::new(), image_data: None,
            size, is_binary: true, is_image: false,
            mime_type: Some(mime.into()),
        },
    }
}

/// Sort a remote tree listing: directories first, then case-insensitive by
/// name. Pure — shared by every provider's `browse_tree`.
pub fn sort_tree(entries: &mut [RemoteTreeEntry]) {
    entries.sort_by(|a, b| {
        let ad = a.entry_type == "dir";
        let bd = b.entry_type == "dir";
        bd.cmp(&ad).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// Map a file path's extension to a MIME type for preview rendering. Pure.
pub fn mime_for_path(path: &str) -> String {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "png"           => "image/png",
        "jpg" | "jpeg"  => "image/jpeg",
        "gif"           => "image/gif",
        "svg"           => "image/svg+xml",
        "webp"          => "image/webp",
        "ico"           => "image/x-icon",
        "bmp"           => "image/bmp",
        "avif"          => "image/avif",
        "rs"            => "text/x-rust",
        "ts" | "tsx"    => "text/typescript",
        "js" | "jsx"    => "text/javascript",
        "svelte"        => "text/plain",
        "vue"           => "text/plain",
        "py"            => "text/x-python",
        "go"            => "text/x-go",
        "java"          => "text/x-java",
        "kt" | "kts"    => "text/x-kotlin",
        "c" | "h"       => "text/x-c",
        "cpp" | "hpp"   => "text/x-c++",
        "cs"            => "text/x-csharp",
        "rb"            => "text/x-ruby",
        "php"           => "text/x-php",
        "sh" | "bash" | "zsh" | "fish" => "text/x-sh",
        "html" | "htm"  => "text/html",
        "css" | "scss" | "sass" | "less" => "text/css",
        "json"          => "application/json",
        "xml"           => "text/xml",
        "sql"           => "text/x-sql",
        "md" | "mdx"    => "text/markdown",
        "toml" | "yaml" | "yml" | "ini" | "cfg" | "conf" | "env" | "lock" => "text/plain",
        "txt" | "log" | "gitignore" | "gitattributes" | "editorconfig"    => "text/plain",
        "pdf"           => "application/pdf",
        "wasm"          => "application/wasm",
        _               => "application/octet-stream",
    }.to_string()
}

#[cfg(test)]
mod file_content_tests {
    use super::*;

    #[test]
    fn mime_by_extension() {
        assert_eq!(mime_for_path("src/main.rs"), "text/x-rust");
        assert_eq!(mime_for_path("logo.PNG"), "image/png");
        assert_eq!(mime_for_path("a/b/c.unknownext"), "application/octet-stream");
        assert_eq!(mime_for_path("README.md"), "text/markdown");
    }

    #[test]
    fn text_file_inlined_as_utf8() {
        let f = build_file_content("a.txt", b"hello".to_vec(), "text/plain");
        assert_eq!(f.content, "hello");
        assert!(!f.is_binary && !f.is_image);
        assert_eq!(f.size, 5);
    }

    #[test]
    fn small_image_inlined_as_data_url() {
        let f = build_file_content("a.png", vec![1, 2, 3], "image/png");
        assert!(f.is_image && !f.is_binary);
        assert_eq!(f.image_data.as_deref(), Some("data:image/png;base64,AQID"));
        assert!(f.content.is_empty());
    }

    #[test]
    fn oversize_text_flagged_binary() {
        let big = vec![b'x'; (MAX_PREVIEW_BYTES + 1) as usize];
        let f = build_file_content("big.txt", big, "text/plain");
        assert!(f.is_binary && f.content.is_empty());
    }

    #[test]
    fn non_utf8_flagged_binary() {
        let f = build_file_content("x.bin", vec![0xff, 0xfe, 0x00], "application/octet-stream");
        assert!(f.is_binary && f.content.is_empty() && !f.is_image);
    }
}
