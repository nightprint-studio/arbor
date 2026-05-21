use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Webhook {
    pub id:     String,
    pub url:    String,
    pub events: Vec<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookCreateRequest {
    pub url:     String,
    pub events:  Vec<String>,
    pub secret:  Option<String>,
    pub active:  bool,
}
