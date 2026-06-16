use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub id:           String,
    pub tag_name:     String,
    pub name:         Option<String>,
    pub body:         Option<String>,
    pub draft:        bool,
    pub prerelease:   bool,
    pub created_at:   String,
    pub published_at: Option<String>,
    pub web_url:      String,
    pub assets:       Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub id:           String,
    pub name:         String,
    pub size_bytes:   u64,
    pub download_url: String,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseCreateRequest {
    pub tag_name:        String,
    pub target_committish: Option<String>,
    pub name:            Option<String>,
    pub body:            Option<String>,
    pub draft:           bool,
    pub prerelease:      bool,
}
