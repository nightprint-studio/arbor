//! SOAP, as far as a client needs it and without an XML library: which protocol version and `SOAPAction` a
//! request uses and the headers they imply, a WS-Security `UsernameToken` put into an envelope, a `Fault`
//! read out of an answer, and an indented view of an XML text.
//!
//! The scanning is deliberately tolerant and small (find a tag, read to its end): envelopes are produced by
//! tools and pasted by people, and the worst a misreading can do is leave an envelope unchanged.

use serde::{Deserialize, Serialize};

use crate::kv::KeyValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SoapVersion {
    #[default]
    #[serde(rename = "1.1")]
    V11,
    #[serde(rename = "1.2")]
    V12,
}

impl SoapVersion {
    pub fn as_str(self) -> &'static str {
        match self {
            SoapVersion::V11 => "1.1",
            SoapVersion::V12 => "1.2",
        }
    }
    pub fn envelope_ns(self) -> &'static str {
        match self {
            SoapVersion::V11 => "http://schemas.xmlsoap.org/soap/envelope/",
            SoapVersion::V12 => "http://www.w3.org/2003/05/soap-envelope",
        }
    }
}

/// What makes a request a SOAP call: the protocol version and the `SOAPAction`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SoapSettings {
    pub version: SoapVersion,
    /// May hold `{{variables}}`; empty for a service that does not use one.
    pub action: String,
}

/// A starting envelope: the request opens with it so there is something to edit.
pub fn sample_envelope(version: SoapVersion, operation: &str, namespace: &str) -> String {
    format!(
        "<soapenv:Envelope xmlns:soapenv=\"{}\" xmlns:tns=\"{namespace}\">\n   <soapenv:Header/>\n   <soapenv:Body>\n      <tns:{operation}>\n      </tns:{operation}>\n   </soapenv:Body>\n</soapenv:Envelope>",
        version.envelope_ns()
    )
}

/// The headers the protocol wants. 1.1 sends `text/xml` and a `SOAPAction` header; 1.2 carries the action in the
/// content type. `action` is already interpolated.
pub fn protocol_headers(version: SoapVersion, action: &str) -> Vec<(String, String)> {
    let action = action.trim().trim_matches('"');
    match version {
        SoapVersion::V11 => vec![("Content-Type".into(), "text/xml; charset=utf-8".into()), ("SOAPAction".into(), format!("\"{action}\""))],
        SoapVersion::V12 => {
            let ct = if action.is_empty() { "application/soap+xml; charset=utf-8".to_string() } else { format!("application/soap+xml; charset=utf-8; action=\"{action}\"") };
            vec![("Content-Type".into(), ct)]
        }
    }
}

/// From a pasted cURL: if the call is SOAP, its settings; the headers the protocol implies are removed from
/// `headers` (the request sets them itself).
pub fn from_headers(headers: &mut Vec<KeyValue>, content_type: &str, data: &str) -> Option<SoapSettings> {
    let action_header = headers.iter().find(|h| h.key.eq_ignore_ascii_case("soapaction")).map(|h| h.value.trim().trim_matches('"').to_string());
    let v12 = content_type.contains("soap+xml");
    let looks_soap = data.contains("Envelope") && (v12 || action_header.is_some() || content_type.contains("xml"));
    if !looks_soap {
        return None;
    }
    let action = if v12 {
        content_type.split("action=").nth(1).map(|a| a.trim().trim_matches(|c| c == '"' || c == ';').to_string()).unwrap_or_default()
    } else {
        action_header.unwrap_or_default()
    };
    headers.retain(|h| !h.key.eq_ignore_ascii_case("soapaction") && !h.key.eq_ignore_ascii_case("content-type"));
    Some(SoapSettings { version: if v12 { SoapVersion::V12 } else { SoapVersion::V11 }, action })
}

// ── A tolerant tag scanner ───────────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct Tag {
    start: usize,
    /// One past the `>`.
    end: usize,
    name: String,
    closing: bool,
    self_closing: bool,
}

impl Tag {
    fn prefix(&self) -> &str {
        self.name.split_once(':').map_or("", |(p, _)| p)
    }
    fn local(&self) -> &str {
        self.name.rsplit(':').next().unwrap_or(&self.name)
    }
}

/// The end (exclusive) of the markup that starts at `at` (`<`): a comment, a CDATA section, or a tag whose
/// quoted attribute values may hold a `>`.
fn markup_end(text: &str, at: usize) -> usize {
    let rest = &text[at..];
    for (open, close) in [("<!--", "-->"), ("<![CDATA[", "]]>")] {
        if rest.starts_with(open) {
            return rest.find(close).map_or(text.len(), |i| at + i + close.len());
        }
    }
    let mut quote: Option<char> = None;
    for (i, c) in rest.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '"') | (None, '\'') => quote = Some(c),
            (None, '>') => return at + i + 1,
            _ => {}
        }
    }
    text.len()
}

fn tags(text: &str) -> Vec<Tag> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = text[at..].find('<') {
        let start = at + i;
        let end = markup_end(text, start);
        let inner = &text[start + 1..end.saturating_sub(1).max(start + 1)];
        let special = inner.starts_with('!') || inner.starts_with('?');
        if !special && !inner.is_empty() {
            let closing = inner.starts_with('/');
            let body = inner.trim_start_matches('/');
            let name: String = body.chars().take_while(|c| !c.is_whitespace() && *c != '/' && *c != '>').collect();
            out.push(Tag { start, end, name, closing, self_closing: inner.ends_with('/') });
        }
        at = end.max(start + 1);
    }
    out
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn strip_tags(text: &str) -> String {
    let mut out = String::new();
    let mut at = 0;
    while let Some(i) = text[at..].find('<') {
        out.push_str(&text[at..at + i]);
        at = markup_end(text, at + i);
    }
    out.push_str(&text[at.min(text.len())..]);
    out
}

/// The text inside the first element called `local` (any prefix), tags stripped, entities decoded.
fn element_text(xml: &str, local: &str) -> Option<String> {
    let all = tags(xml);
    let open = all.iter().position(|t| !t.closing && !t.self_closing && t.local() == local)?;
    let close = all[open + 1..].iter().find(|t| t.closing && t.local() == local)?;
    Some(unescape(strip_tags(&xml[all[open].end..close.start]).trim()))
}

// ── WS-Security ──────────────────────────────────────────────────────────────────────────────────

const WSSE_NS: &str = "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd";
const PASSWORD_TEXT: &str = "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordText";

/// The envelope with a WS-Security `UsernameToken` (plain-text password, over TLS) in its Header. An envelope
/// that already has a `Security` header, or is not an envelope, is returned unchanged.
pub fn add_username_token(envelope: &str, username: &str, password: &str) -> String {
    let all = tags(envelope);
    let Some(env) = all.iter().find(|t| !t.closing && t.local() == "Envelope") else { return envelope.to_string() };
    if all.iter().any(|t| !t.closing && t.local() == "Security") {
        return envelope.to_string();
    }
    let must_understand = if env.prefix().is_empty() { String::new() } else { format!(" {}:mustUnderstand=\"1\"", env.prefix()) };
    let security = format!(
        "<wsse:Security xmlns:wsse=\"{WSSE_NS}\"{must_understand}><wsse:UsernameToken><wsse:Username>{}</wsse:Username><wsse:Password Type=\"{PASSWORD_TEXT}\">{}</wsse:Password></wsse:UsernameToken></wsse:Security>",
        escape(username),
        escape(password)
    );
    let header = all.iter().find(|t| t.start >= env.end && t.local() == "Header" && !t.closing);
    let mut out = envelope.to_string();
    match header {
        Some(h) if h.self_closing => {
            // `<s:Header/>` → `<s:Header>…</s:Header>`
            let open = format!("<{}>", h.name);
            out.replace_range(h.start..h.end, &format!("{open}{security}</{}>", h.name));
        }
        Some(h) => out.insert_str(h.end, &security),
        None => {
            let name = if env.prefix().is_empty() { "Header".to_string() } else { format!("{}:Header", env.prefix()) };
            out.insert_str(env.end, &format!("<{name}>{security}</{name}>"));
        }
    }
    out
}

// ── A Fault ──────────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Fault {
    pub code: String,
    pub message: String,
    pub detail: String,
}

/// The fault in an answer, 1.1 (`faultcode`/`faultstring`/`detail`) or 1.2 (`Code/Value`, `Reason/Text`,
/// `Detail`); `None` when the answer is not a fault.
pub fn fault_of(body: &str) -> Option<Fault> {
    let all = tags(body);
    let at = all.iter().position(|t| !t.closing && t.local() == "Fault")?;
    let fault = &body[all[at].start..];
    let first = |names: &[&str]| names.iter().find_map(|n| element_text(fault, n)).unwrap_or_default();
    Some(Fault { code: first(&["faultcode", "Value"]), message: first(&["faultstring", "Text"]), detail: first(&["detail", "Detail"]) })
}

// ── Reading an answer ────────────────────────────────────────────────────────────────────────────

/// An indented copy of an XML text: one tag per line, an element that holds only text kept on one line.
/// Text it cannot make sense of is returned as it came.
pub fn pretty_xml(text: &str) -> String {
    let all = tags(text);
    if all.is_empty() {
        return text.to_string();
    }
    enum Piece<'a> {
        Tag(&'a Tag),
        Text(String),
        Other(String),
    }
    let mut pieces: Vec<Piece> = Vec::new();
    let mut known = all.iter().peekable();
    // Interleave the tags with the text and the other markup (comments, declarations) between them.
    let mut cursor = 0;
    while cursor < text.len() {
        let Some(i) = text[cursor..].find('<') else {
            let rest = text[cursor..].trim();
            if !rest.is_empty() {
                pieces.push(Piece::Text(rest.to_string()));
            }
            break;
        };
        let start = cursor + i;
        let between = text[cursor..start].trim();
        if !between.is_empty() {
            pieces.push(Piece::Text(between.to_string()));
        }
        let end = markup_end(text, start);
        match known.next_if(|t| t.start == start) {
            Some(t) => pieces.push(Piece::Tag(t)),
            None => pieces.push(Piece::Other(text[start..end].to_string())),
        }
        cursor = end.max(start + 1);
    }

    let mut out = String::new();
    let mut depth: usize = 0;
    let indent = |d: usize| "   ".repeat(d);
    let raw = |t: &Tag| text[t.start..t.end].to_string();
    let mut i = 0;
    while i < pieces.len() {
        match &pieces[i] {
            Piece::Tag(t) if !t.closing && !t.self_closing => {
                // <a>text</a> stays on one line.
                if let (Some(Piece::Text(inner)), Some(Piece::Tag(c))) = (pieces.get(i + 1), pieces.get(i + 2)) {
                    if c.closing && c.name == t.name {
                        out.push_str(&format!("{}{}{}{}\n", indent(depth), raw(t), inner, raw(c)));
                        i += 3;
                        continue;
                    }
                }
                out.push_str(&format!("{}{}\n", indent(depth), raw(t)));
                depth += 1;
            }
            Piece::Tag(t) if t.closing => {
                depth = depth.saturating_sub(1);
                out.push_str(&format!("{}{}\n", indent(depth), raw(t)));
            }
            Piece::Tag(t) => out.push_str(&format!("{}{}\n", indent(depth), raw(t))),
            Piece::Text(s) | Piece::Other(s) => out.push_str(&format!("{}{}\n", indent(depth), s)),
        }
        i += 1;
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENVELOPE: &str = "<soapenv:Envelope xmlns:soapenv=\"http://schemas.xmlsoap.org/soap/envelope/\"><soapenv:Header/><soapenv:Body><a:Op/></soapenv:Body></soapenv:Envelope>";

    #[test]
    fn soap_11_sends_text_xml_and_a_quoted_action_header() {
        let h = protocol_headers(SoapVersion::V11, "urn:GetWeather");
        assert_eq!(h[0], ("Content-Type".to_string(), "text/xml; charset=utf-8".to_string()));
        assert_eq!(h[1], ("SOAPAction".to_string(), "\"urn:GetWeather\"".to_string()));
    }

    #[test]
    fn soap_12_carries_the_action_in_the_content_type() {
        let h = protocol_headers(SoapVersion::V12, "urn:x");
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].1, "application/soap+xml; charset=utf-8; action=\"urn:x\"");
        assert_eq!(protocol_headers(SoapVersion::V12, "")[0].1, "application/soap+xml; charset=utf-8");
    }

    #[test]
    fn a_username_token_goes_into_an_empty_header() {
        let out = add_username_token(ENVELOPE, "bob", "p&ss");
        assert!(out.contains("<soapenv:Header><wsse:Security"), "{out}");
        assert!(out.contains("soapenv:mustUnderstand=\"1\""));
        assert!(out.contains("<wsse:Username>bob</wsse:Username>"));
        assert!(out.contains(">p&amp;ss</wsse:Password>"), "the password is escaped");
        assert!(out.ends_with("</soapenv:Envelope>"));
    }

    #[test]
    fn a_username_token_joins_a_header_that_has_content_and_is_never_added_twice() {
        let with = ENVELOPE.replace("<soapenv:Header/>", "<soapenv:Header><x:Trace/></soapenv:Header>");
        let out = add_username_token(&with, "u", "p");
        assert!(out.contains("<soapenv:Header><wsse:Security"), "{out}");
        assert!(out.contains("<x:Trace/>"));
        assert_eq!(add_username_token(&out, "u", "p"), out);
    }

    #[test]
    fn a_missing_header_is_created_and_a_non_envelope_is_left_alone() {
        let bare = "<s:Envelope xmlns:s=\"x\"><s:Body/></s:Envelope>";
        assert!(add_username_token(bare, "u", "p").starts_with("<s:Envelope xmlns:s=\"x\"><s:Header><wsse:Security"));
        assert_eq!(add_username_token("<html/>", "u", "p"), "<html/>");
    }

    #[test]
    fn a_soap_11_fault_is_read() {
        let body = "<s:Envelope xmlns:s=\"x\"><s:Body><s:Fault><faultcode>s:Server</faultcode><faultstring>Boom &amp; bust</faultstring><detail><e:Info>stack</e:Info></detail></s:Fault></s:Body></s:Envelope>";
        let f = fault_of(body).unwrap();
        assert_eq!((f.code.as_str(), f.message.as_str(), f.detail.as_str()), ("s:Server", "Boom & bust", "stack"));
    }

    #[test]
    fn a_soap_12_fault_is_read_and_a_normal_answer_is_not_a_fault() {
        let body = "<env:Envelope xmlns:env=\"x\"><env:Body><env:Fault><env:Code><env:Value>env:Receiver</env:Value></env:Code><env:Reason><env:Text xml:lang=\"en\">Down</env:Text></env:Reason></env:Fault></env:Body></env:Envelope>";
        let f = fault_of(body).unwrap();
        assert_eq!((f.code.as_str(), f.message.as_str()), ("env:Receiver", "Down"));
        assert!(fault_of("<s:Envelope><s:Body><r>ok</r></s:Body></s:Envelope>").is_none());
    }

    #[test]
    fn a_pasted_curl_with_a_soap_action_is_recognised_and_its_headers_absorbed() {
        let mut headers = vec![KeyValue::new("SOAPAction", "\"urn:op\""), KeyValue::new("Content-Type", "text/xml"), KeyValue::new("X-Team", "it")];
        let s = from_headers(&mut headers, "text/xml", ENVELOPE).unwrap();
        assert_eq!((s.version, s.action.as_str()), (SoapVersion::V11, "urn:op"));
        assert_eq!(headers, vec![KeyValue::new("X-Team", "it")]);
        let mut h12 = vec![KeyValue::new("Content-Type", "application/soap+xml; charset=utf-8; action=\"urn:z\"")];
        assert_eq!(from_headers(&mut h12, "application/soap+xml; charset=utf-8; action=\"urn:z\"", ENVELOPE).unwrap().action, "urn:z");
        assert!(from_headers(&mut vec![], "application/xml", "<a/>").is_none());
    }

    #[test]
    fn xml_is_indented_and_text_only_elements_stay_on_one_line() {
        let out = pretty_xml("<a><b>1</b><c><d/></c></a>");
        assert_eq!(out, "<a>\n   <b>1</b>\n   <c>\n      <d/>\n   </c>\n</a>");
    }

    #[test]
    fn what_is_not_xml_comes_back_as_it_was() {
        assert_eq!(pretty_xml("plain text"), "plain text");
    }
}

/// The SOAP rules across the layers: what is sent, what is saved, what a pasted cURL becomes.
#[cfg(test)]
mod across_the_model {
    use super::*;
    use crate::auth::Auth;
    use crate::curl::{parse_curl, to_curl};
    use crate::format::{request_from_text, request_to_text};
    use crate::method::Method;
    use crate::prepare::{prepare, Inherited};
    use crate::request::{Body, BodyKind, HttpRequest};
    use crate::vars::ResolvedVars;

    fn call() -> HttpRequest {
        HttpRequest {
            name: "GetWeather".into(),
            method: Method::Post,
            url: "http://svc.example/ws".into(),
            body: Body { kind: BodyKind::Soap, text: sample_envelope(SoapVersion::V11, "GetWeather", "http://example.com/w"), form: vec![] },
            soap: SoapSettings { version: SoapVersion::V11, action: "urn:GetWeather".into() },
            ..Default::default()
        }
    }

    #[test]
    fn a_soap_request_is_sent_with_the_protocol_headers() {
        let p = prepare(&call(), &Inherited::default(), &ResolvedVars::default());
        assert!(p.headers.contains(&("Content-Type".to_string(), "text/xml; charset=utf-8".to_string())));
        assert!(p.headers.contains(&("SOAPAction".to_string(), "\"urn:GetWeather\"".to_string())));
        assert!(p.body.unwrap().contains("<tns:GetWeather>"));
    }

    #[test]
    fn ws_security_from_the_request_or_the_level_above_goes_into_the_envelope() {
        let mut own = call();
        own.auth = Auth::WsSecurity { username: "bob".into(), password: "pw".into() };
        let body = prepare(&own, &Inherited::default(), &ResolvedVars::default()).body.unwrap();
        assert!(body.contains("<wsse:Username>bob</wsse:Username>"), "{body}");

        let above = Auth::WsSecurity { username: "carol".into(), password: "x".into() };
        let inherited = Inherited { auth: vec![&above], headers: vec![] };
        let body = prepare(&call(), &inherited, &ResolvedVars::default()).body.unwrap();
        assert!(body.contains("<wsse:Username>carol</wsse:Username>"), "{body}");
        assert!(!prepare(&own, &Inherited::default(), &ResolvedVars::default()).headers.iter().any(|(k, _)| k == "Authorization"));
    }

    #[test]
    fn a_soap_request_survives_the_file() {
        let mut r = call();
        r.auth = Auth::WsSecurity { username: "bob".into(), password: "{{pw}}".into() };
        r.soap.version = SoapVersion::V12;
        let back = request_from_text(&request_to_text(&r)).unwrap();
        assert_eq!(back, r);
        assert!(request_to_text(&r).contains("[soap]\nversion = \"1.2\""));
    }

    #[test]
    fn copy_as_curl_carries_the_soap_headers_and_pasting_it_back_gives_the_same_call() {
        let curl = to_curl(&prepare(&call(), &Inherited::default(), &ResolvedVars::default()));
        assert!(curl.contains("SOAPAction"), "{curl}");
        let back = parse_curl(&curl).unwrap();
        assert_eq!(back.body.kind, BodyKind::Soap);
        assert_eq!((back.soap.version, back.soap.action.as_str()), (SoapVersion::V11, "urn:GetWeather"));
        assert!(back.headers.is_empty(), "the protocol headers are the request's own again: {:?}", back.headers);
    }
}
