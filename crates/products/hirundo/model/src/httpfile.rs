//! `.http` / `.rest` files — the plain-text request format IntelliJ's HTTP Client made common — read into
//! requests and written back from them, plus the two things that make such a file runnable: the dynamic
//! variables (`{{$uuid}}`…) and the environment files (`http-client.env.json`).
//!
//! ```text
//! ### Login                         a block per request; the text after ### is its title
//! # @name login                     a comment may carry directives
//! POST {{host}}/api/login            [METHOD] URL [HTTP/1.1] — the method defaults to GET
//!   ?remember=true                   an indented line continues the URL
//! Content-Type: application/json     headers, until a blank line
//!
//! { "user": "bob" }                  the body — or `< ./body.json` to take it from a file
//!
//! > {% client.global.set("t", response.body.token); %}     a response handler (JavaScript)
//! > ./last-response.json             where to save the response
//! ```
//!
//! What is **not** carried over: JavaScript response handlers (`> {% … %}`) and `<>` response references are read
//! and reported as warnings, never run. Hirundo's own tests (assertions and Lua) travel in comments —
//! `# @hirundo-test status == 200`, `# @hirundo-lua …` — which IntelliJ ignores and Hirundo restores.

use serde::Serialize;
use serde_json::Value;

use crate::auth::{ApiKeyIn, Auth};
use crate::env::Environment;
use crate::kv::{enabled_pairs, parse_text, KeyValue};
use crate::method::Method;
use crate::request::{split_query, Body, BodyKind, HttpRequest};
use crate::soap;

const HTTP_BOUNDARY: &str = "HirundoBoundary";
const TEST_MARK: &str = "@hirundo-test ";
const LUA_MARK: &str = "@hirundo-lua ";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HttpEntry {
    /// The text after `###`, or the request's `@name`.
    pub title: String,
    pub request: HttpRequest,
    /// `< ./file`: the body is to be read from this path (relative to the `.http` file); the body text is empty until then.
    pub body_file: Option<String>,
    /// Where the response is to be saved (`> path`), which Hirundo does not do.
    pub save_to: Option<String>,
    /// What was read but cannot be carried over, in words.
    pub warnings: Vec<String>,
    /// 1-based lines in the file: where the block starts (its `###` line), the request line, and where the block ends.
    pub start_line: usize,
    pub request_line: usize,
    pub end_line: usize,
}

// ── Reading ──────────────────────────────────────────────────────────────────────────────────────

fn is_comment(line: &str) -> bool {
    let t = line.trim_start();
    (t.starts_with('#') && !t.starts_with("###")) || t.starts_with("//")
}

fn comment_text(line: &str) -> &str {
    line.trim_start().trim_start_matches(['#', '/']).trim()
}

/// The blocks of a file, each with its title (the text after `###`) and its lines. Text before the first
/// separator is a block of its own (with no title).
fn blocks(text: &str) -> Vec<(String, usize, Vec<&str>)> {
    // (title, 1-based number of the first content line, lines)
    let mut out: Vec<(String, usize, Vec<&str>)> = vec![(String::new(), 1, Vec::new())];
    for (i, line) in text.lines().enumerate() {
        if let Some(rest) = line.trim_start().strip_prefix("###") {
            out.push((rest.trim_matches(|c: char| c == '#' || c.is_whitespace()).to_string(), i + 2, Vec::new()));
        } else if let Some((_, _, lines)) = out.last_mut() {
            lines.push(line);
        }
    }
    out
}

/// A `@name = value` line: a file variable (IntelliJ), not a request.
fn file_variable(line: &str) -> Option<KeyValue> {
    let rest = line.trim().strip_prefix('@')?;
    let (name, value) = rest.split_once('=')?;
    let name = name.trim();
    (!name.is_empty() && !name.contains(char::is_whitespace)).then(|| KeyValue::new(name, value.trim()))
}

/// The `@name = value` variables of a file, in order (a later one replaces an earlier one of the same name).
pub fn file_variables(text: &str) -> Vec<KeyValue> {
    let mut out: Vec<KeyValue> = Vec::new();
    for v in text.lines().filter_map(file_variable) {
        match out.iter_mut().find(|r| r.key == v.key) {
            Some(row) => row.value = v.value,
            None => out.push(v),
        }
    }
    out
}

fn entry_of(title: &str, first_line: usize, lines: &[&str]) -> Option<HttpEntry> {
    let mut warnings = Vec::new();
    let mut name: Option<String> = None;
    let mut tests: Vec<String> = Vec::new();
    let mut lua: Vec<String> = Vec::new();

    // Comments (and blank lines) before the request line.
    let mut i = 0;
    while i < lines.len() && (lines[i].trim().is_empty() || is_comment(lines[i]) || file_variable(lines[i]).is_some()) {
        let c = comment_text(lines[i]);
        if let Some(n) = c.strip_prefix("@name") {
            name = Some(n.trim().to_string());
        } else if let Some(t) = c.strip_prefix(TEST_MARK.trim_end()) {
            tests.push(t.trim().to_string());
        } else if let Some(l) = lines[i].trim_start().trim_start_matches(['#', '/']).trim_start().strip_prefix(LUA_MARK.trim_end()) {
            lua.push(l.strip_prefix(' ').unwrap_or(l).to_string());
        }
        i += 1;
    }
    let first = lines.get(i)?;
    let request_line = first_line + i;

    // The request line: [METHOD] URL [HTTP/x.y].
    let mut words = first.split_whitespace();
    let head = words.next()?;
    let (method, url_word) = match Method::parse(head) {
        Some(m) => (m, words.next().unwrap_or("")),
        None => (Method::Get, head),
    };
    let mut url = url_word.to_string();
    i += 1;
    // Indented lines go on with the URL (`  ?a=1`, `  &b=2`).
    while i < lines.len() && lines[i].starts_with([' ', '\t']) && !lines[i].trim().is_empty() {
        url.push_str(lines[i].trim());
        i += 1;
    }

    // Headers, until a blank line.
    let mut headers: Vec<KeyValue> = Vec::new();
    while i < lines.len() && !lines[i].trim().is_empty() {
        let line = lines[i];
        if line.trim_start().starts_with('>') || line.trim_start().starts_with("<>") {
            break;
        }
        match line.split_once(':') {
            Some((k, v)) if !k.trim().is_empty() && !k.contains(' ') => headers.push(KeyValue::new(k.trim(), v.trim())),
            _ => break,
        }
        i += 1;
    }

    // The body and what follows it.
    let mut body_lines: Vec<&str> = Vec::new();
    let mut body_file = None;
    let mut save_to = None;
    let mut handler = false;
    while i < lines.len() {
        let line = lines[i];
        let t = line.trim_start();
        if t.starts_with("> {%") || t.starts_with(">{%") {
            handler = true;
            // Skip to the closing `%}`.
            while i < lines.len() && !lines[i].contains("%}") {
                i += 1;
            }
        } else if let Some(rest) = t.strip_prefix(">>!").or_else(|| t.strip_prefix(">>")).or_else(|| t.strip_prefix('>')) {
            save_to = Some(rest.trim().to_string());
        } else if t.starts_with("<>") {
            warnings.push("A `<>` response reference was ignored.".to_string());
        } else if body_lines.iter().all(|l| l.trim().is_empty()) && t.starts_with("< ") {
            body_file = Some(t[2..].trim().to_string());
        } else {
            body_lines.push(line);
        }
        i += 1;
    }
    if handler {
        warnings.push("A JavaScript response handler (`> {% … %}`) was ignored: Hirundo's tests are assertions and Lua.".to_string());
    }
    if save_to.is_some() {
        warnings.push("Saving the response to a file (`> path`) is not supported.".to_string());
    }
    while body_lines.first().is_some_and(|l| l.trim().is_empty()) {
        body_lines.remove(0);
    }
    while body_lines.last().is_some_and(|l| l.trim().is_empty()) {
        body_lines.pop();
    }
    let data = body_lines.join("\n");

    let (path, params) = split_query(&url);
    let content_type = headers.iter().find(|h| h.key.eq_ignore_ascii_case("content-type")).map(|h| h.value.to_ascii_lowercase()).unwrap_or_default();
    let mut soap_settings = soap::SoapSettings::default();
    let body = if data.is_empty() {
        Body::default()
    } else if let Some(found) = soap::from_headers(&mut headers, &content_type, &data) {
        soap_settings = found;
        Body { kind: BodyKind::Soap, text: data, form: Vec::new() }
    } else if content_type.contains("json") || (content_type.is_empty() && data.trim_start().starts_with(['{', '['])) {
        headers.retain(|h| !(h.key.eq_ignore_ascii_case("content-type") && h.value.to_ascii_lowercase().starts_with("application/json")));
        Body { kind: BodyKind::Json, text: data, form: Vec::new() }
    } else if content_type.contains("xml") {
        Body { kind: BodyKind::Xml, text: data, form: Vec::new() }
    } else if content_type.contains("x-www-form-urlencoded") {
        headers.retain(|h| !h.key.eq_ignore_ascii_case("content-type"));
        Body { kind: BodyKind::Form, text: String::new(), form: parse_text(&data) }
    } else {
        Body { kind: BodyKind::Text, text: data, form: Vec::new() }
    };

    let mut test_text = tests.join("\n");
    if !lua.is_empty() {
        test_text = format!("{test_text}\n--- lua\n{}", lua.join("\n")).trim_start().to_string();
    }
    let display = name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| if title.is_empty() { format!("{} {}", method.as_str(), path.rsplit('/').find(|s| !s.is_empty()).unwrap_or(&path)) } else { title.to_string() });
    Some(HttpEntry {
        title: display.clone(),
        request: HttpRequest { name: display, method, url: path, params, headers, body, soap: soap_settings, tests: test_text, ..HttpRequest::default() },
        body_file,
        save_to,
        warnings,
        start_line: first_line.saturating_sub(1).max(1),
        request_line,
        end_line: first_line + lines.len().saturating_sub(1),
    })
}

/// Every request in a `.http` text, in order. Blocks with no request line (only comments) are skipped.
pub fn parse_http_file(text: &str) -> Vec<HttpEntry> {
    blocks(text).iter().filter_map(|(title, first, lines)| entry_of(title, *first, lines)).collect()
}

// ── Writing ──────────────────────────────────────────────────────────────────────────────────────

fn query_of(r: &HttpRequest, extra: &[(String, String)]) -> String {
    let pairs: Vec<(String, String)> = enabled_pairs(&r.params).into_iter().chain(extra.iter().cloned()).collect();
    if pairs.is_empty() {
        return String::new();
    }
    format!("?{}", pairs.iter().map(|(k, v)| if v.is_empty() { k.clone() } else { format!("{k}={v}") }).collect::<Vec<_>>().join("&"))
}

fn request_to_block(r: &HttpRequest) -> String {
    let mut out = format!("### {}\n", r.name);
    if !r.docs.trim().is_empty() {
        for line in r.docs.lines().take(3) {
            out.push_str(&format!("# {line}\n"));
        }
    }
    let (declarative, lua) = crate::assertion::split_lua(&r.tests);
    for line in declarative.lines().map(str::trim).filter(|l| !l.is_empty()) {
        out.push_str(&format!("# {TEST_MARK}{line}\n"));
    }
    for line in lua.iter().flat_map(|l| l.lines()) {
        out.push_str(&format!("# {LUA_MARK}{line}\n"));
    }

    let mut headers: Vec<(String, String)> = enabled_pairs(&r.headers);
    let mut extra_query = Vec::new();
    match &r.auth {
        Auth::Bearer { token } => headers.push(("Authorization".into(), format!("Bearer {token}"))),
        Auth::Basic { username, password } => headers.push(("Authorization".into(), format!("Basic {username} {password}"))),
        Auth::ApiKey { name, value, location: ApiKeyIn::Header } => headers.push((name.clone(), value.clone())),
        Auth::ApiKey { name, value, location: ApiKeyIn::Query } => extra_query.push((name.clone(), value.clone())),
        Auth::WsSecurity { .. } | Auth::Inherit | Auth::NoAuth => {}
    }
    let has = |headers: &[(String, String)], name: &str| headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name));
    let body = match r.body.kind {
        BodyKind::Empty => None,
        BodyKind::Json => Some((r.body.text.clone(), "application/json".to_string())),
        BodyKind::Text => Some((r.body.text.clone(), "text/plain".to_string())),
        BodyKind::Xml => Some((r.body.text.clone(), "application/xml".to_string())),
        BodyKind::Form => Some((enabled_pairs(&r.body.form).iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&"), "application/x-www-form-urlencoded".to_string())),
        BodyKind::Multipart => {
            // IntelliJ's shape: a boundary in the content type, one block per part, a file as `< ./path`.
            if !has(&headers, "Content-Type") {
                headers.push(("Content-Type".into(), format!("multipart/form-data; boundary={HTTP_BOUNDARY}")));
            }
            let mut text = String::new();
            for part in crate::multipart::parts_of(&r.body.form, &mut |s| s.to_string()) {
                text.push_str(&format!("--{HTTP_BOUNDARY}\nContent-Disposition: form-data; name=\"{}\"", part.name));
                if let Some(file) = &part.file {
                    text.push_str(&format!("; filename=\"{}\"", crate::multipart::file_name(file)));
                }
                text.push('\n');
                if let Some(ct) = part.content_type.clone().or_else(|| part.file.as_deref().map(|f| crate::multipart::guess_content_type(f).to_string())) {
                    text.push_str(&format!("Content-Type: {ct}\n"));
                }
                text.push('\n');
                text.push_str(&part.file.as_ref().map_or(part.value.clone(), |f| format!("< {f}")));
                text.push('\n');
            }
            text.push_str(&format!("--{HTTP_BOUNDARY}--"));
            Some((text, String::new()))
        }
        BodyKind::Soap => {
            for (k, v) in soap::protocol_headers(r.soap.version, &r.soap.action) {
                if !has(&headers, &k) {
                    headers.push((k, v));
                }
            }
            Some((r.body.text.clone(), String::new()))
        }
    };
    if let Some((_, ct)) = &body {
        if !ct.is_empty() && !has(&headers, "Content-Type") {
            headers.push(("Content-Type".into(), ct.clone()));
        }
    }

    out.push_str(&format!("{} {}{}\n", r.method.as_str(), r.url, query_of(r, &extra_query)));
    for (k, v) in &headers {
        out.push_str(&format!("{k}: {v}\n"));
    }
    if let Some((text, _)) = body {
        out.push_str(&format!("\n{text}\n"));
    }
    out
}

/// Requests as a `.http` file, one block each.
pub fn to_http_file(requests: &[HttpRequest]) -> String {
    requests.iter().map(request_to_block).collect::<Vec<_>>().join("\n")
}

// ── Dynamic variables ────────────────────────────────────────────────────────────────────────────

/// The `{{$name}}` placeholders a request uses, once each.
pub fn dynamic_names(request: &HttpRequest) -> Vec<String> {
    let mut texts: Vec<&str> = vec![&request.url, &request.body.text, &request.soap.action];
    for row in request.params.iter().chain(&request.headers).chain(&request.body.form) {
        texts.push(&row.key);
        texts.push(&row.value);
    }
    let mut names: Vec<String> = Vec::new();
    for text in texts {
        let mut rest = text;
        while let Some(start) = rest.find("{{") {
            let after = &rest[start + 2..];
            let Some(end) = after.find("}}") else { break };
            let name = after[..end].trim();
            if name.starts_with('$') && !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
            rest = &after[end + 2..];
        }
    }
    names
}

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The value of one dynamic variable. `now_ms` and `entropy` come from the caller (the model has no clock and
/// no randomness): `$uuid`, `$timestamp` (seconds), `$isoTimestamp`, `$randomInt` (0–999) and
/// `$random.integer(from, to)`. `None` for a name it does not know.
pub fn dynamic_value(name: &str, now_ms: i64, entropy: u64) -> Option<String> {
    let mut state = entropy;
    match name {
        "$uuid" | "$random.uuid" => {
            let (a, b) = (splitmix(&mut state), splitmix(&mut state));
            let hex = format!("{a:016x}{b:016x}");
            Some(format!("{}-{}-4{}-{:x}{}-{}", &hex[0..8], &hex[8..12], &hex[13..16], 8 + (splitmix(&mut state) % 4), &hex[17..20], &hex[20..32]))
        }
        "$timestamp" => Some((now_ms / 1000).to_string()),
        "$isoTimestamp" => Some(iso_from_ms(now_ms)),
        "$randomInt" => Some((splitmix(&mut state) % 1000).to_string()),
        other => {
            let args = other.strip_prefix("$random.integer(")?.strip_suffix(')')?;
            let (from, to) = args.split_once(',')?;
            let (from, to): (i64, i64) = (from.trim().parse().ok()?, to.trim().parse().ok()?);
            let (lo, hi) = (from.min(to), from.max(to));
            Some((lo + (splitmix(&mut state) % ((hi - lo + 1) as u64)) as i64).to_string())
        }
    }
}

/// `2026-11-04T09:30:00Z` from unix milliseconds (civil-from-days, no calendar crate).
fn iso_from_ms(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// The dynamic variables a request uses, with values, as the runtime layer of the variable cascade.
pub fn dynamic_vars(request: &HttpRequest, now_ms: i64, entropy: u64) -> Vec<KeyValue> {
    dynamic_names(request)
        .into_iter()
        .enumerate()
        .filter_map(|(i, name)| dynamic_value(&name, now_ms, entropy.wrapping_add(i as u64 * 0x1234_5678_9ABC)).map(|v| KeyValue::new(&name, &v)))
        .collect()
}

// ── Diagnostics ──────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diag {
    /// 1-based line, and 0-based character columns of the span.
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub error: bool,
    pub message: String,
}

const DYNAMIC: [&str; 5] = ["$uuid", "$random.uuid", "$timestamp", "$isoTimestamp", "$randomInt"];

fn is_dynamic(name: &str) -> bool {
    DYNAMIC.contains(&name) || (name.starts_with("$random.integer(") && name.ends_with(')'))
}

/// What is wrong in a `.http` text that can be said without sending it: a request with no URL, a variable
/// nobody defines (not in the file, not in `known`, not a dynamic one).
pub fn diagnose(text: &str, known: &[String]) -> Vec<Diag> {
    let mut out = Vec::new();
    let file_vars: Vec<String> = file_variables(text).into_iter().map(|v| v.key).collect();
    for (n, line) in text.lines().enumerate() {
        let lineno = n + 1;
        if file_variable(line).is_some() {
            continue;
        }
        // Variables, anywhere a request is (comments excluded).
        if !is_comment(line) {
            let mut from = 0;
            while let Some(open) = line[from..].find("{{") {
                let start = from + open;
                let Some(close) = line[start..].find("}}") else { break };
                let name = line[start + 2..start + close].trim();
                let (cs, ce) = (line[..start].chars().count(), line[..start + close + 2].chars().count());
                if name.starts_with("$") {
                    if !is_dynamic(name) {
                        out.push(Diag { line: lineno, start: cs, end: ce, error: false, message: format!("unknown dynamic variable `{name}`") });
                    }
                } else if !name.is_empty() && !file_vars.iter().any(|v| v == name) && !known.iter().any(|v| v == name) {
                    out.push(Diag { line: lineno, start: cs, end: ce, error: false, message: format!("variable `{name}` is not defined") });
                }
                from = start + close + 2;
            }
        }
    }
    for entry in parse_http_file(text) {
        if entry.request.url.trim().is_empty() {
            let width = text.lines().nth(entry.request_line.saturating_sub(1)).map_or(0, |l| l.chars().count());
            out.push(Diag { line: entry.request_line, start: 0, end: width, error: true, message: "the request has no URL".to_string() });
        }
    }
    out.sort_by_key(|d| (d.line, d.start));
    out
}

// ── Environment files ────────────────────────────────────────────────────────────────────────────

fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// `http-client.env.json` (and, merged over it, `http-client.private.env.json`) as environments: one per
/// top-level key, `$shared` merged into each, a type guessed from the name (`dev`, `local`, `staging`, `prod`).
pub fn parse_http_env(public: &str, private: Option<&str>) -> Result<Vec<Environment>, String> {
    let read = |text: &str, what: &str| -> Result<serde_json::Map<String, Value>, String> {
        match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(o)) => Ok(o),
            Ok(_) => Err(format!("{what} must be a JSON object of environments")),
            Err(e) => Err(format!("{what} is not valid JSON: {e}")),
        }
    };
    let public = read(public, "http-client.env.json")?;
    let private = private.map(|p| read(p, "http-client.private.env.json")).transpose()?.unwrap_or_default();

    let mut names: Vec<String> = Vec::new();
    for key in public.keys().chain(private.keys()) {
        if key != "$shared" && !names.contains(key) {
            names.push(key.clone());
        }
    }

    let vars_of = |name: &str| -> Vec<KeyValue> {
        let mut vars: Vec<KeyValue> = Vec::new();
        for source in [public.get("$shared"), private.get("$shared"), public.get(name), private.get(name)].into_iter().flatten().filter_map(Value::as_object) {
            for (k, v) in source {
                match vars.iter_mut().find(|r| r.key == *k) {
                    Some(row) => row.value = scalar(v),
                    None => vars.push(KeyValue::new(k, &scalar(v))),
                }
            }
        }
        vars
    };
    Ok(names
        .into_iter()
        .map(|name| {
            let lower = name.to_ascii_lowercase();
            let type_id = ["local", "dev", "staging", "prod"]
                .into_iter()
                .find(|t| lower.contains(t) || (*t == "staging" && (lower.contains("stage") || lower.contains("test"))) || (*t == "prod" && lower.contains("production")))
                .unwrap_or("")
                .to_string();
            Environment { id: crate::slug::slugify(&name), vars: vars_of(&name), name, type_id }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "\
### Login
# @name login
# @hirundo-test status == 200
POST {{host}}/api/login
  ?remember=true
Content-Type: application/json

{ \"user\": \"bob\" }

> {% client.global.set(\"t\", response.body.token); %}

###
// no title: named after the path
GET {{host}}/api/items/42
Accept: application/json
Authorization: Bearer {{token}}

### Form
POST {{host}}/form
Content-Type: application/x-www-form-urlencoded

a=1&b=two

### From file
PUT {{host}}/up
Content-Type: application/json

< ./body.json
> ./out.json
";

    #[test]
    fn a_file_is_read_block_by_block() {
        let entries = parse_http_file(FILE);
        assert_eq!(entries.len(), 4);
        let login = &entries[0].request;
        assert_eq!((login.name.as_str(), login.method, login.url.as_str()), ("login", Method::Post, "{{host}}/api/login"));
        assert_eq!(login.params, vec![KeyValue::new("remember", "true")], "the indented line continued the URL");
        assert_eq!((login.body.kind, login.body.text.as_str()), (BodyKind::Json, "{ \"user\": \"bob\" }"));
        assert!(login.headers.is_empty(), "the JSON content type is implied by the body kind");
        assert_eq!(login.tests, "status == 200");
        assert!(entries[0].warnings.iter().any(|w| w.contains("JavaScript")));
    }

    #[test]
    fn the_method_defaults_to_get_and_a_missing_title_is_made_from_the_path() {
        let get = &parse_http_file(FILE)[1].request;
        assert_eq!((get.method, get.name.as_str()), (Method::Get, "GET 42"));
        assert_eq!(get.headers.len(), 2);
    }

    #[test]
    fn form_bodies_and_body_files_are_understood() {
        let entries = parse_http_file(FILE);
        assert_eq!(entries[2].request.body.kind, BodyKind::Form);
        assert_eq!(entries[2].request.body.form, vec![KeyValue::new("a", "1"), KeyValue::new("b", "two")]);
        assert_eq!(entries[3].body_file.as_deref(), Some("./body.json"));
        assert_eq!(entries[3].save_to.as_deref(), Some("./out.json"));
        assert!(entries[3].warnings.iter().any(|w| w.contains("Saving the response")));
    }

    #[test]
    fn a_soap_call_is_recognised_by_its_action() {
        let text = "### Soap\nPOST http://svc/ws\nContent-Type: text/xml\nSOAPAction: \"urn:Op\"\n\n<s:Envelope xmlns:s=\"x\"><s:Body/></s:Envelope>\n";
        let r = &parse_http_file(text)[0].request;
        assert_eq!((r.body.kind, r.soap.action.as_str()), (BodyKind::Soap, "urn:Op"));
    }

    #[test]
    fn requests_survive_a_round_trip_through_the_file() {
        let original: Vec<HttpRequest> = parse_http_file(FILE).into_iter().take(3).map(|e| e.request).collect();
        let text = to_http_file(&original);
        let back: Vec<HttpRequest> = parse_http_file(&text).into_iter().map(|e| e.request).collect();
        assert_eq!(back.len(), 3);
        for (a, b) in original.iter().zip(&back) {
            assert_eq!((&a.name, a.method, &a.url, &a.params, &a.body.kind, &a.body.text, &a.tests), (&b.name, b.method, &b.url, &b.params, &b.body.kind, &b.body.text, &b.tests));
        }
    }

    #[test]
    fn auth_is_written_as_the_header_intellij_reads() {
        let r = HttpRequest { name: "a".into(), url: "http://x".into(), auth: Auth::Bearer { token: "{{t}}".into() }, ..Default::default() };
        assert!(to_http_file(&[r]).contains("Authorization: Bearer {{t}}"));
    }

    #[test]
    fn hirundo_tests_and_lua_travel_in_comments() {
        let r = HttpRequest { name: "t".into(), url: "http://x".into(), tests: "status == 200\n--- lua\nhirundo.expect(true)".into(), ..Default::default() };
        let text = to_http_file(&[r.clone()]);
        assert!(text.contains("# @hirundo-test status == 200") && text.contains("# @hirundo-lua hirundo.expect(true)"));
        assert_eq!(parse_http_file(&text)[0].request.tests, r.tests);
    }

    #[test]
    fn dynamic_variables_are_found_and_valued() {
        let r = HttpRequest { url: "http://x/{{$uuid}}?t={{ $timestamp }}".into(), body: Body { kind: BodyKind::Text, text: "{{$randomInt}} {{$uuid}} {{name}}".into(), form: vec![] }, ..Default::default() };
        assert_eq!(dynamic_names(&r), ["$uuid", "$timestamp", "$randomInt"]);
        let vars = dynamic_vars(&r, 1_762_248_600_000, 7);
        assert_eq!(vars.len(), 3);
        let uuid = &vars[0].value;
        assert_eq!(uuid.len(), 36);
        assert_eq!(&uuid[14..15], "4", "version 4");
        assert_eq!(vars[1].value, "1762248600");
        assert!(vars[2].value.parse::<u32>().unwrap() < 1000);
        assert_eq!(dynamic_value("$uuid", 0, 7), dynamic_value("$uuid", 0, 7), "same entropy, same value");
        assert_ne!(dynamic_value("$uuid", 0, 7), dynamic_value("$uuid", 0, 8));
        let n: i64 = dynamic_value("$random.integer(5, 9)", 0, 3).unwrap().parse().unwrap();
        assert!((5..=9).contains(&n));
        assert_eq!(dynamic_value("$nope", 0, 0), None);
    }

    #[test]
    fn the_iso_timestamp_is_a_real_date() {
        assert_eq!(iso_from_ms(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_from_ms(1_762_248_600_000), "2025-11-04T09:30:00Z");
    }

    #[test]
    fn intellij_environment_files_become_environments() {
        let public = r#"{ "$shared": { "api": "v1" }, "dev": { "host": "localhost:8080", "token": "" }, "production": { "host": "https://api.example.com" } }"#;
        let private = r#"{ "dev": { "token": "secret" } }"#;
        let envs = parse_http_env(public, Some(private)).unwrap();
        assert_eq!(envs.iter().map(|e| (e.id.as_str(), e.type_id.as_str())).collect::<Vec<_>>(), [("dev", "dev"), ("production", "prod")]);
        let dev = &envs[0];
        assert_eq!(dev.vars.iter().map(|v| (v.key.as_str(), v.value.as_str())).collect::<Vec<_>>(), [("api", "v1"), ("host", "localhost:8080"), ("token", "secret")], "private beats public, $shared first");
        assert!(parse_http_env("[1]", None).unwrap_err().contains("object"));
        assert!(parse_http_env("{", None).unwrap_err().contains("not valid JSON"));
    }

    #[test]
    fn entries_know_their_lines_and_file_variables_are_not_requests() {
        let text = "@host = http://x\n@token=abc\n\n### One\nGET {{host}}/a\n\n### Two\n# note\nPOST {{host}}/b\n";
        let entries = parse_http_file(text);
        assert_eq!(entries.len(), 2, "the @ lines before the first ### are variables, not a request");
        assert_eq!((entries[0].start_line, entries[0].request_line, entries[0].end_line), (4, 5, 6));
        assert_eq!((entries[1].start_line, entries[1].request_line), (7, 9));
        assert_eq!(file_variables(text), vec![KeyValue::new("host", "http://x"), KeyValue::new("token", "abc")]);
    }

    #[test]
    fn a_request_with_no_url_and_an_undefined_variable_are_reported_where_they_are() {
        let text = "@a = 1\n### x\nGET\n\n### y\nGET {{a}}/{{b}}/{{$uuid}}/{{$nope}}\n";
        let d = diagnose(text, &["known".to_string()]);
        assert!(d.iter().any(|x| x.error && x.line == 3 && x.message.contains("no URL")), "{d:?}");
        let b = d.iter().find(|x| x.message.contains("`b`")).unwrap();
        assert_eq!((b.line, b.start, b.end, b.error), (6, 10, 15, false));
        assert!(d.iter().any(|x| x.message.contains("$nope")));
        assert!(!d.iter().any(|x| x.message.contains("`a`") || x.message.contains("$uuid")), "defined and dynamic variables are fine");
        assert!(diagnose("### z\nGET {{known}}/x\n", &["known".to_string()]).is_empty());
    }
}
