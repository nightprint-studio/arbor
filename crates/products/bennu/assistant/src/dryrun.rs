//! A request, shown instead of sent.
//!
//! The dry run is the assistant's answer to "what exactly would you tell the model?": the endpoint, the
//! headers, the JSON body, and where its size comes from — built by the **same** code that builds the
//! real request, so what is shown is what would go, not a description of it. Because building a request
//! is pure (see [`crate::provider::Provider::body`]), nothing here touches the network and nothing needs
//! a key; where the credential would sit it shows a masked stand-in, never the value.
//!
//! The breakdown is an estimate — four characters to a token — and says so: the real count comes back
//! with the provider's answer.

use serde_json::Value;

use crate::message::Request;
use crate::provider::Provider;

/// What the request would be, and what is in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Preview {
    pub provider: String,
    pub method: &'static str,
    pub url: String,
    /// Every header, the credential's included — with its value masked.
    pub headers: Vec<(String, String)>,
    pub body: Value,
    pub breakdown: Breakdown,
}

/// Where the request's size is: estimated tokens by part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Breakdown {
    /// The standing instructions.
    pub system: u64,
    /// The tool definitions — the same on every request, which is what makes them cacheable.
    pub tools: u64,
    /// The conversation so far, tool calls and their results included.
    pub messages: u64,
}

impl Breakdown {
    pub fn total(&self) -> u64 {
        self.system + self.tools + self.messages
    }
}

/// `sk-ant-api03-abcdef…wxyz` → `sk-ant-…wxyz`: enough to tell which key it is, never enough to use it.
pub fn mask_key(key: &str) -> String {
    let key = key.trim();
    if key.is_empty() {
        return "(no key set)".to_string();
    }
    let n = key.chars().count();
    if n <= 12 {
        return "••••••••".to_string();
    }
    let head: String = key.chars().take(7).collect();
    let tail: String = key.chars().skip(n - 4).collect();
    format!("{head}…{tail}")
}

/// Four characters to a token, rounded up — close enough to steer by.
fn tokens_of(value: &Value) -> u64 {
    (value.to_string().chars().count() as u64).div_ceil(4)
}

/// Build what `provider` would send for `request`, with the credential masked. `key` is only ever
/// looked at to be masked, and may be empty.
pub fn preview(provider: &dyn Provider, request: &Request, key: &str) -> Preview {
    let endpoint = provider.endpoint();
    let (auth_name, _) = provider.auth_header(key);
    let mut headers = endpoint.headers;
    headers.push((auth_name, mask_key(key)));

    let body = provider.body(request);
    let part = |name: &str| body.get(name).map(tokens_of).unwrap_or(0);
    let breakdown = Breakdown { system: part("system"), tools: part("tools"), messages: part("messages") };

    Preview {
        provider: provider.id().to_string(),
        method: "POST",
        url: endpoint.url,
        headers,
        body,
        breakdown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anthropic::Anthropic;
    use crate::effort::Effort;
    use crate::message::{Message, ToolSpec};
    use serde_json::json;

    fn request() -> Request {
        Request {
            model: "claude-opus-5-5".into(),
            system: "You are Bennu's assistant.".into(),
            messages: vec![Message::user("rename total to amountDue")],
            tools: vec![ToolSpec { name: "rename".into(), description: "Rename a symbol".into(), input_schema: json!({"type": "object"}) }],
            max_tokens: 4096,
            effort: Some(Effort::High),
        }
    }

    #[test]
    fn the_preview_is_the_body_the_provider_would_send() {
        let p = preview(&Anthropic, &request(), "sk-ant-api03-secretsecretsecret-abcd");
        assert_eq!(p.body, Anthropic.body(&request()), "the same code builds both");
        assert_eq!(p.method, "POST");
        assert!(p.url.ends_with("/v1/messages"));
        assert_eq!(p.provider, "anthropic");
    }

    #[test]
    fn the_key_is_masked_and_never_appears() {
        let key = "sk-ant-api03-secretsecretsecret-abcd";
        let p = preview(&Anthropic, &request(), key);
        let auth = p.headers.iter().find(|(k, _)| k == "x-api-key").expect("the credential header is listed");
        assert_eq!(auth.1, "sk-ant-…abcd");
        let everything = format!("{:?}", p);
        assert!(!everything.contains("secretsecret"), "no part of the middle of the key is kept");
    }

    #[test]
    fn without_a_key_it_says_so() {
        let p = preview(&Anthropic, &request(), "");
        let auth = p.headers.iter().find(|(k, _)| k == "x-api-key").unwrap();
        assert_eq!(auth.1, "(no key set)");
    }

    #[test]
    fn a_short_key_shows_nothing_of_itself() {
        assert_eq!(mask_key("abc123"), "••••••••");
    }

    #[test]
    fn the_breakdown_counts_each_part_and_adds_up() {
        let p = preview(&Anthropic, &request(), "");
        assert!(p.breakdown.system > 0 && p.breakdown.tools > 0 && p.breakdown.messages > 0);
        assert_eq!(p.breakdown.total(), p.breakdown.system + p.breakdown.tools + p.breakdown.messages);
    }

    #[test]
    fn a_bigger_conversation_costs_more_tokens() {
        let small = preview(&Anthropic, &request(), "").breakdown.messages;
        let mut r = request();
        r.messages.push(Message::user("x".repeat(4000)));
        let big = preview(&Anthropic, &r, "").breakdown.messages;
        assert!(big >= small + 1000, "4000 characters is about 1000 tokens more");
    }
}
