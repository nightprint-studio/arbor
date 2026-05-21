//! `arbor.http.get` worker.
//!
//! Issues a single GET via reqwest, captures status + body + headers (small
//! allow-list) into a JSON value the Lua callback consumes verbatim. Errors
//! are folded into the same envelope so the Lua side never has to branch on
//! "result vs exception" — `payload.ok` is the single source of truth.

pub(crate) async fn perform_http_get(
    url: &str,
    headers: &[(String, String)],
    timeout_ms: u64,
) -> serde_json::Value {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .user_agent("arbor-plugin/1.0")
        .build()
    {
        Ok(c) => c,
        Err(e) => return serde_json::json!({
            "ok": false, "status": 0, "body": "",
            "error": format!("client build failed: {e}"),
        }),
    };
    let mut req = client.get(url);
    for (k, v) in headers { req = req.header(k, v); }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => return serde_json::json!({
            "ok": false, "status": 0, "body": "",
            "error": format!("request failed: {e}"),
        }),
    };
    let status = resp.status().as_u16();
    let ok     = resp.status().is_success();
    match resp.text().await {
        Ok(body) => serde_json::json!({
            "ok": ok, "status": status, "body": body,
        }),
        Err(e) => serde_json::json!({
            "ok": false, "status": status, "body": "",
            "error": format!("body read failed: {e}"),
        }),
    }
}
