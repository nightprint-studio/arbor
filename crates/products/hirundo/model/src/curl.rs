//! cURL in both directions: paste `curl …` (or Chrome's "Copy as cURL") and get a request; copy a request
//! as `curl …`.
//!
//! Reading is faithful to what curl does, not to what is convenient: without `-L` curl does not follow
//! redirects, so the imported request does not either; `-d` makes a `POST`; `-G` turns the data into a
//! query. Shell quoting is read as bash writes it — `'…'`, `"…"` and `$'…'` — and a backslash at the end
//! of a line continues the command. The Windows `cmd` flavour of "Copy as cURL" (carets everywhere) is not
//! read; copy the bash one.

use base64::Engine;

use crate::auth::Auth;
use crate::kv::{parse_text, percent_encode, KeyValue};
use crate::method::Method;
use crate::prepare::Prepared;
use crate::request::{split_query, Body, BodyKind, HttpRequest, Settings};
use crate::soap::{self, SoapSettings};
use crate::multipart;

/// Split a command line into words the way a POSIX shell does (without expanding anything).
pub fn tokenize(input: &str) -> Result<Vec<String>, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut started = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' if chars.get(i + 1) == Some(&'\n') => i += 1,
            '\\' if chars.get(i + 1) == Some(&'\r') && chars.get(i + 2) == Some(&'\n') => i += 2,
            '\\' => {
                started = true;
                if let Some(&n) = chars.get(i + 1) {
                    cur.push(n);
                    i += 1;
                }
            }
            '\'' => {
                started = true;
                let end = chars[i + 1..].iter().position(|&x| x == '\'').ok_or("an unclosed ' in the command")?;
                cur.extend(&chars[i + 1..i + 1 + end]);
                i += end + 1;
            }
            '$' if chars.get(i + 1) == Some(&'\'') => {
                started = true;
                i += 2;
                loop {
                    match chars.get(i) {
                        None => return Err("an unclosed $' in the command".into()),
                        Some('\'') => break,
                        Some('\\') => {
                            i += 1;
                            match chars.get(i) {
                                Some('n') => cur.push('\n'),
                                Some('t') => cur.push('\t'),
                                Some('r') => cur.push('\r'),
                                Some('x') => {
                                    let hex: String = chars[i + 1..].iter().take(2).take_while(|c| c.is_ascii_hexdigit()).collect();
                                    if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                                        cur.push(ch);
                                    }
                                    i += hex.len();
                                }
                                Some('u') => {
                                    let hex: String = chars[i + 1..].iter().take(4).take_while(|c| c.is_ascii_hexdigit()).collect();
                                    if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                                        cur.push(ch);
                                    }
                                    i += hex.len();
                                }
                                Some(&other) => cur.push(other),
                                None => return Err("an unclosed $' in the command".into()),
                            }
                        }
                        Some(&other) => cur.push(other),
                    }
                    i += 1;
                }
            }
            '"' => {
                started = true;
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return Err("an unclosed \" in the command".into()),
                        Some('"') => break,
                        Some('\\') => match chars.get(i + 1) {
                            Some(&n) if matches!(n, '"' | '\\' | '$' | '`') => {
                                cur.push(n);
                                i += 1;
                            }
                            Some('\n') => i += 1,
                            _ => cur.push('\\'),
                        },
                        Some(&other) => cur.push(other),
                    }
                    i += 1;
                }
            }
            c if c.is_whitespace() => {
                if started {
                    tokens.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            c => {
                started = true;
                cur.push(c);
            }
        }
        i += 1;
    }
    if started {
        tokens.push(cur);
    }
    Ok(tokens)
}

/// Options that take a value, short and long, that Hirundo has no use for: read and dropped.
const IGNORED_WITH_VALUE: [&str; 13] = ["-o", "--output", "-w", "--write-out", "--connect-timeout", "-x", "--proxy", "--cacert", "--cert", "--key", "-T", "--retry", "--resolve"];
/// Options without a value that change nothing a request is made of.
const IGNORED_FLAGS: [&str; 18] = [
    "-s", "--silent", "-S", "--show-error", "-v", "--verbose", "-i", "--include", "--compressed", "-g", "--globoff", "-N", "--no-buffer", "-f", "--fail", "-#", "--http1.1", "--http2",
];

#[derive(Default)]
struct Parsed {
    method: Option<Method>,
    url: Option<String>,
    headers: Vec<(String, String)>,
    data: Vec<String>,
    user: Option<String>,
    get: bool,
    insecure: bool,
    follow: bool,
    max_time_ms: Option<u64>,
    /// `-F` arguments, in order.
    forms: Vec<String>,
}

fn data_urlencode(part: &str) -> String {
    match part.split_once('=') {
        Some((name, value)) if !name.is_empty() => format!("{name}={}", percent_encode(value)),
        Some((_, value)) => percent_encode(value),
        None => percent_encode(part),
    }
}

fn apply_option(p: &mut Parsed, opt: &str, value: Option<String>) -> Result<(), String> {
    let need = |v: Option<String>| v.ok_or_else(|| format!("{opt} needs a value"));
    match opt {
        "-X" | "--request" => p.method = Method::parse(&need(value)?).or(p.method),
        "--url" => p.url = Some(need(value)?),
        "-H" | "--header" => {
            let h = need(value)?;
            if let Some((k, v)) = h.split_once(':') {
                p.headers.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
        "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-ascii" => p.data.push(need(value)?),
        "--data-urlencode" => p.data.push(data_urlencode(&need(value)?)),
        "-u" | "--user" => p.user = Some(need(value)?),
        "-A" | "--user-agent" => p.headers.push(("User-Agent".into(), need(value)?)),
        "-e" | "--referer" => p.headers.push(("Referer".into(), need(value)?)),
        "-b" | "--cookie" => p.headers.push(("Cookie".into(), need(value)?)),
        "-m" | "--max-time" => {
            let secs: f64 = need(value)?.parse().map_err(|_| "--max-time needs a number of seconds".to_string())?;
            p.max_time_ms = Some((secs * 1000.0) as u64);
        }
        "-I" | "--head" => p.method = Some(Method::Head),
        "-G" | "--get" => p.get = true,
        "-k" | "--insecure" => p.insecure = true,
        "-L" | "--location" => p.follow = true,
        "-F" | "--form" | "--form-string" => p.forms.push(need(value)?),
        o if IGNORED_FLAGS.contains(&o) => {}
        o if IGNORED_WITH_VALUE.contains(&o) => {
            need(value)?;
        }
        other => return Err(format!("the option {other} is not understood")),
    }
    Ok(())
}

/// Whether an option takes the next word as its value.
fn takes_value(opt: &str) -> bool {
    matches!(
        opt,
        "-X" | "--request" | "--url" | "-H" | "--header" | "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-ascii" | "--data-urlencode" | "-u" | "--user" | "-A" | "--user-agent" | "-e"
            | "--referer" | "-b" | "--cookie" | "-m" | "--max-time" | "-F" | "--form" | "--form-string"
    ) || IGNORED_WITH_VALUE.contains(&opt)
}

/// Turn `curl …` into a request. Errors say which part could not be understood.
pub fn parse_curl(text: &str) -> Result<HttpRequest, String> {
    let mut tokens = tokenize(text.trim())?.into_iter().peekable();
    if !tokens.peek().is_some_and(|t| t.eq_ignore_ascii_case("curl")) {
        return Err("this does not start with `curl`".into());
    }
    tokens.next();
    let mut p = Parsed::default();
    while let Some(tok) = tokens.next() {
        if tok.starts_with("--") && tok.len() > 2 {
            let value = if takes_value(&tok) { tokens.next() } else { None };
            apply_option(&mut p, &tok, value)?;
        } else if tok.starts_with('-') && tok.len() > 1 {
            // `-sSL` is three flags; `-XPOST` is `-X POST`.
            let body: Vec<char> = tok.chars().skip(1).collect();
            let mut k = 0;
            while k < body.len() {
                let opt = format!("-{}", body[k]);
                if takes_value(&opt) {
                    let attached: String = body[k + 1..].iter().collect();
                    let value = if attached.is_empty() { tokens.next() } else { Some(attached) };
                    apply_option(&mut p, &opt, value)?;
                    break;
                }
                apply_option(&mut p, &opt, None)?;
                k += 1;
            }
        } else if p.url.is_none() {
            p.url = Some(tok);
        } else {
            return Err(format!("unexpected extra argument '{tok}'"));
        }
    }
    build(p)
}

fn build(p: Parsed) -> Result<HttpRequest, String> {
    let url = p.url.ok_or("there is no URL in the command")?;
    let mut data = p.data.join("&");
    let mut full_url = url;
    if p.get && !data.is_empty() {
        full_url.push(if full_url.contains('?') { '&' } else { '?' });
        full_url.push_str(&data);
        data.clear();
    }
    let (path, mut params) = split_query(&full_url);
    let method = p.method.unwrap_or(if data.is_empty() && p.forms.is_empty() { Method::Get } else { Method::Post });

    let mut headers: Vec<KeyValue> = Vec::new();
    let mut auth = Auth::Inherit;
    let mut content_type = String::new();
    for (k, v) in p.headers {
        if k.eq_ignore_ascii_case("authorization") {
            if let Some(token) = v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")) {
                auth = Auth::Bearer { token: token.trim().to_string() };
                continue;
            }
            if let Some(b64) = v.strip_prefix("Basic ").or_else(|| v.strip_prefix("basic ")) {
                let decoded = base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok().and_then(|b| String::from_utf8(b).ok());
                if let Some((u, pw)) = decoded.as_deref().and_then(|d| d.split_once(':')) {
                    auth = Auth::Basic { username: u.to_string(), password: pw.to_string() };
                    continue;
                }
            }
        }
        if k.eq_ignore_ascii_case("content-type") {
            content_type = v.to_ascii_lowercase();
        }
        headers.push(KeyValue::new(&k, &v));
    }
    if let Some(user) = p.user {
        let (u, pw) = user.split_once(':').unwrap_or((user.as_str(), ""));
        auth = Auth::Basic { username: u.to_string(), password: pw.to_string() };
    }

    let mut soap = SoapSettings::default();
    let form_rows: Vec<KeyValue> = p.forms.iter().filter_map(|f| multipart::row_from_curl(f)).collect();
    let body = if !form_rows.is_empty() {
        Body { kind: BodyKind::Multipart, text: String::new(), form: form_rows }
    } else if data.is_empty() {
        Body::default()
    } else if let Some(found) = soap::from_headers(&mut headers, &content_type, &data) {
        soap = found;
        Body { kind: BodyKind::Soap, text: data, form: vec![] }
    } else if content_type.contains("json") {
        headers.retain(|h| !h.key.eq_ignore_ascii_case("content-type"));
        Body { kind: BodyKind::Json, text: data, form: vec![] }
    } else if content_type.contains("xml") {
        Body { kind: BodyKind::Xml, text: data, form: vec![] }
    } else if content_type.contains("x-www-form-urlencoded") || (content_type.is_empty() && data.contains('=') && !data.trim_start().starts_with(['{', '[', '<'])) {
        headers.retain(|h| !h.key.eq_ignore_ascii_case("content-type"));
        Body { kind: BodyKind::Form, text: String::new(), form: parse_text(&data) }
    } else if content_type.is_empty() && data.trim_start().starts_with(['{', '[']) {
        Body { kind: BodyKind::Json, text: data, form: vec![] }
    } else {
        Body { kind: BodyKind::Text, text: data, form: vec![] }
    };

    // Params read out of a query in the pasted URL keep their order after any `-G` data.
    params.retain(|r| !r.key.is_empty());
    let name = {
        let tail = path.split("://").nth(1).unwrap_or(&path).split(['?', '#']).next().unwrap_or("");
        let segments: Vec<&str> = tail.split('/').filter(|s| !s.is_empty()).collect();
        let what = if segments.len() > 1 { segments[segments.len() - 1] } else { segments.first().copied().unwrap_or("request") };
        format!("{} {}", method.as_str(), what)
    };
    let defaults = Settings::default();
    Ok(HttpRequest {
        name,
        method,
        url: path,
        params,
        headers,
        auth,
        body,
        soap,
        settings: Settings { timeout_ms: p.max_time_ms.unwrap_or(defaults.timeout_ms), follow_redirects: p.follow, verify_tls: !p.insecure, ..defaults },
        ..Default::default()
    })
}

/// A word safe to paste into a POSIX shell: single-quoted, with `'` written as `'\''`.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// A prepared request as a `curl` command, one option per line.
pub fn to_curl(p: &Prepared) -> String {
    let mut lines = vec![format!("curl {}", quote(&p.url))];
    match p.method {
        Method::Get => {}
        Method::Head => lines.push("-I".into()),
        m => lines.push(format!("-X {}", m.as_str())),
    }
    for (k, v) in &p.headers {
        lines.push(format!("-H {}", quote(&format!("{k}: {v}"))));
    }
    if let Some(body) = &p.body {
        lines.push(format!("--data-raw {}", quote(body)));
    }
    if let Some(parts) = &p.multipart {
        for flag in multipart::curl_flags(parts) {
            lines.push(format!("-F {}", quote(&flag)));
        }
    }
    if p.follow_redirects {
        lines.push("-L".into());
    }
    if !p.verify_tls {
        lines.push("--insecure".into());
    }
    if p.timeout_ms != crate::request::DEFAULT_TIMEOUT_MS {
        lines.push(format!("--max-time {}", p.timeout_ms as f64 / 1000.0));
    }
    lines.join(" \\\n  ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prepare::{prepare, Inherited};
    use crate::vars::ResolvedVars;

    #[test]
    fn the_shell_quotes_are_read_the_way_bash_writes_them() {
        assert_eq!(tokenize(r#"curl 'it'\''s' "a \"b\" c" plain"#).unwrap(), ["curl", "it's", "a \"b\" c", "plain"]);
        assert_eq!(tokenize("a \\\n  b").unwrap(), ["a", "b"], "a backslash-newline continues the line");
        assert_eq!(tokenize("--data-raw $'{\"a\":\"x\\ny\"}' -s").unwrap(), ["--data-raw", "{\"a\":\"x\ny\"}", "-s"]);
        assert_eq!(tokenize("''").unwrap(), [""], "an empty quoted word is a word");
        assert!(tokenize("curl 'open").is_err());
    }

    #[test]
    fn chromes_copy_as_curl_becomes_a_request() {
        let r = parse_curl(
            "curl 'https://api.example.it/api/bandi?stato=APERTO&page=0' \\\n  -H 'accept: application/json' \\\n  -H 'authorization: Bearer abc.def' \\\n  -H 'content-type: application/json' \\\n  --data-raw '{\"titolo\":\"Nuovo\"}' \\\n  --compressed",
        )
        .unwrap();
        assert_eq!(r.method, Method::Post, "data makes it a POST");
        assert_eq!(r.url, "https://api.example.it/api/bandi");
        assert_eq!(r.params, vec![KeyValue::new("stato", "APERTO"), KeyValue::new("page", "0")]);
        assert_eq!(r.auth, Auth::Bearer { token: "abc.def".into() }, "the Authorization header became auth");
        assert_eq!(r.headers, vec![KeyValue::new("accept", "application/json")], "and the JSON content type is implied by the body kind");
        assert_eq!((r.body.kind, r.body.text.as_str()), (BodyKind::Json, "{\"titolo\":\"Nuovo\"}"));
        assert!(!r.settings.follow_redirects, "no -L: curl would not follow");
        assert_eq!(r.name, "POST bandi");
    }

    #[test]
    fn short_flags_combine_and_values_attach() {
        let r = parse_curl("curl -sSLk -XPUT -u mario:s3 https://x.it/a/b/c --max-time 2.5").unwrap();
        assert_eq!(r.method, Method::Put);
        assert_eq!(r.auth, Auth::Basic { username: "mario".into(), password: "s3".into() });
        assert!(r.settings.follow_redirects && !r.settings.verify_tls);
        assert_eq!(r.settings.timeout_ms, 2500);
    }

    #[test]
    fn dash_g_turns_the_data_into_a_query_and_head_is_head() {
        let r = parse_curl("curl -G -d 'q=a b' -d page=2 https://x.it/s").unwrap();
        assert_eq!((r.method, r.body.kind), (Method::Get, BodyKind::Empty));
        assert_eq!(r.params, vec![KeyValue::new("q", "a b"), KeyValue::new("page", "2")]);
        assert_eq!(parse_curl("curl -I https://x.it").unwrap().method, Method::Head);
    }

    #[test]
    fn form_data_becomes_form_rows_and_basic_auth_in_a_header_is_decoded() {
        let r = parse_curl("curl https://x.it/login -d 'user=mario&pw=a%26b' -H 'Authorization: Basic YWxhZGRpbjpvcGVuc2VzYW1l'").unwrap();
        assert_eq!(r.body.kind, BodyKind::Form);
        assert_eq!(r.body.form, vec![KeyValue::new("user", "mario"), KeyValue::new("pw", "a&b")]);
        assert_eq!(r.auth, Auth::Basic { username: "aladdin".into(), password: "opensesame".into() });
    }

    #[test]
    fn what_cannot_be_read_says_so() {
        assert!(parse_curl("wget http://x").unwrap_err().contains("curl"));
        assert!(parse_curl("curl").unwrap_err().contains("no URL"));
        assert!(parse_curl("curl --frobnicate http://x").unwrap_err().contains("--frobnicate"));
        assert!(parse_curl("curl http://a http://b").unwrap_err().contains("extra argument"));
    }

    #[test]
    fn a_request_is_written_as_curl_and_read_back() {
        let original = parse_curl("curl -X PATCH 'https://x.it/a?b=1' -H 'X-Team: it'\\''s' --data-raw '{\"a\":1}' -H 'content-type: application/json' -L --insecure").unwrap();
        let text = to_curl(&prepare(&original, &Inherited::default(), &ResolvedVars::default()));
        assert!(text.starts_with("curl 'https://x.it/a?b=1' \\\n  -X PATCH"), "{text}");
        assert!(text.contains("-H 'X-Team: it'\\''s'"), "{text}");
        let back = parse_curl(&text).unwrap();
        assert_eq!(back.method, Method::Patch);
        assert_eq!(back.url, "https://x.it/a");
        assert_eq!(back.body, original.body);
        assert!(back.settings.follow_redirects && !back.settings.verify_tls);
        assert!(back.headers.iter().any(|h| h.key == "X-Team" && h.value == "it's"));
    }

    #[test]
    fn a_plain_get_is_just_the_url() {
        let r = HttpRequest { url: "http://x/ping".into(), settings: Settings { follow_redirects: false, ..Default::default() }, ..Default::default() };
        assert_eq!(to_curl(&prepare(&r, &Inherited::default(), &ResolvedVars::default())), "curl 'http://x/ping'");
    }

    #[test]
    fn without_variables_the_placeholders_stay_in_the_copied_command() {
        let r = HttpRequest { url: "{{baseUrl}}/ping".into(), settings: Settings { follow_redirects: false, ..Default::default() }, ..Default::default() };
        assert_eq!(to_curl(&prepare(&r, &Inherited::default(), &ResolvedVars::default())), "curl '{{baseUrl}}/ping'");
    }
}
