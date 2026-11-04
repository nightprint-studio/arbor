//! `multipart/form-data`: the rows of a form body turned into parts, and the bytes of the body they make.
//!
//! The editor keeps a multipart body as ordinary key/value rows (the same rows a urlencoded form uses): the key
//! is the field name; a value starting with `@` is a **file** (`@/path/photo.png`, as `curl -F` writes it),
//! anything else is text; the description column holds the part's content type (empty: `text/plain` for text, a
//! guess from the extension for a file). Nothing here reads a disk or a clock: the sender reads the files and
//! hands their bytes to [`encode`], with a [`boundary_for`] seed of its own.

use crate::kv::KeyValue;

/// A part after the variables are filled in, before any file is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormPart {
    pub name: String,
    /// The text of a text part (empty for a file).
    pub value: String,
    /// The path of a file part.
    pub file: Option<String>,
    /// Explicit content type, if the row names one.
    pub content_type: Option<String>,
}

/// A part ready to be written: the file already read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encoded {
    pub name: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub data: Vec<u8>,
}

/// The enabled rows of a multipart body as parts. `fill` interpolates variables.
pub fn parts_of(rows: &[KeyValue], fill: &mut dyn FnMut(&str) -> String) -> Vec<FormPart> {
    rows.iter()
        .filter(|r| r.enabled && !r.key.trim().is_empty())
        .map(|r| {
            let value = fill(&r.value);
            let content_type = Some(fill(&r.description)).filter(|c| !c.trim().is_empty());
            match value.strip_prefix('@') {
                Some(path) if !path.is_empty() => FormPart { name: fill(&r.key), value: String::new(), file: Some(path.to_string()), content_type },
                _ => FormPart { name: fill(&r.key), value, file: None, content_type },
            }
        })
        .collect()
}

/// The file name a part is sent under: the last segment of its path.
pub fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

/// A content type for a file, from its extension; `application/octet-stream` when it is not a common one.
pub fn guess_content_type(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().map(str::to_ascii_lowercase).unwrap_or_default();
    match ext.as_str() {
        "json" => "application/json",
        "xml" => "application/xml",
        "txt" | "log" | "md" | "csv" => "text/plain",
        "html" | "htm" => "text/html",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

/// A boundary that no part is likely to contain, from a seed the caller varies.
pub fn boundary_for(entropy: u64) -> String {
    format!("----HirundoBoundary{entropy:016x}")
}

pub fn content_type_header(boundary: &str) -> String {
    format!("multipart/form-data; boundary={boundary}")
}

fn quote(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"").replace(['\r', '\n'], " ")
}

/// The body: each part behind `--boundary`, headers, a blank line, the data, and the closing `--boundary--`.
pub fn encode(parts: &[Encoded], boundary: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for part in parts {
        out.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{}\"", quote(&part.name)).as_bytes());
        if let Some(file) = &part.filename {
            out.extend_from_slice(format!("; filename=\"{}\"", quote(file)).as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        if let Some(ct) = &part.content_type {
            out.extend_from_slice(format!("Content-Type: {ct}\r\n").as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&part.data);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    out
}

/// `-F` options for `curl`: `name=value` for text, `name=@path;type=…` for a file.
pub fn curl_flags(parts: &[FormPart]) -> Vec<String> {
    parts
        .iter()
        .map(|p| match &p.file {
            Some(path) => format!("{}=@{path}{}", p.name, p.content_type.as_ref().map(|t| format!(";type={t}")).unwrap_or_default()),
            None => format!("{}={}{}", p.name, p.value, p.content_type.as_ref().map(|t| format!(";type={t}")).unwrap_or_default()),
        })
        .collect()
}

/// One `-F` argument (`name=value`, `name=@file;type=image/png`) as a row of a multipart body.
pub fn row_from_curl(spec: &str) -> Option<KeyValue> {
    let (name, rest) = spec.split_once('=')?;
    if name.trim().is_empty() {
        return None;
    }
    let (value, content_type) = match rest.split_once(";type=") {
        Some((v, t)) => (v, t.split(';').next().unwrap_or("").trim()),
        None => (rest, ""),
    };
    // `name=<file` reads a text file's content into the field; here it is a file part like `@`.
    let value = value.strip_prefix('<').map_or_else(|| value.to_string(), |p| format!("@{p}"));
    Some(KeyValue { key: name.trim().to_string(), value, description: content_type.to_string(), enabled: true })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<KeyValue> {
        vec![
            KeyValue::new("title", "Bando"),
            KeyValue { description: "image/png".into(), ..KeyValue::new("logo", "@/tmp/logo.png") },
            KeyValue { enabled: false, ..KeyValue::new("off", "x") },
            KeyValue::new("", "no name"),
        ]
    }

    #[test]
    fn rows_become_text_and_file_parts_and_disabled_or_nameless_ones_are_dropped() {
        let parts = parts_of(&rows(), &mut |s| s.to_string());
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], FormPart { name: "title".into(), value: "Bando".into(), file: None, content_type: None });
        assert_eq!((parts[1].file.as_deref(), parts[1].content_type.as_deref()), (Some("/tmp/logo.png"), Some("image/png")));
    }

    #[test]
    fn variables_are_filled_in_names_values_and_paths() {
        let r = vec![KeyValue::new("{{f}}", "@{{dir}}/a.txt")];
        let parts = parts_of(&r, &mut |s| s.replace("{{f}}", "doc").replace("{{dir}}", "/d"));
        assert_eq!((parts[0].name.as_str(), parts[0].file.as_deref()), ("doc", Some("/d/a.txt")));
    }

    #[test]
    fn the_body_is_framed_by_the_boundary_with_crlf() {
        let body = encode(
            &[
                Encoded { name: "title".into(), filename: None, content_type: None, data: b"Bando".to_vec() },
                Encoded { name: "logo".into(), filename: Some("logo.png".into()), content_type: Some("image/png".into()), data: vec![0, 255, 10] },
            ],
            "XX",
        );
        let text = String::from_utf8_lossy(&body).into_owned();
        assert!(text.starts_with("--XX\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nBando\r\n--XX\r\n"), "{text:?}");
        assert!(text.contains("name=\"logo\"; filename=\"logo.png\"\r\nContent-Type: image/png\r\n\r\n"));
        assert!(body.windows(5).any(|w| w == [0, 255, 10, b'\r', b'\n']), "binary data is written as it is");
        assert!(text.ends_with("--XX--\r\n"));
    }

    #[test]
    fn quotes_and_line_breaks_in_a_name_cannot_break_the_header() {
        let body = encode(&[Encoded { name: "a\"b\nc".into(), filename: None, content_type: None, data: vec![] }], "B");
        assert!(String::from_utf8_lossy(&body).contains("name=\"a\\\"b c\""));
    }

    #[test]
    fn content_types_come_from_the_extension_and_the_boundary_from_the_seed() {
        assert_eq!(guess_content_type("x.PNG"), "image/png");
        assert_eq!(guess_content_type("x.bin"), "application/octet-stream");
        assert_eq!(file_name("C:\\a\\b.txt"), "b.txt");
        assert_eq!(boundary_for(255), "----HirundoBoundary00000000000000ff");
        assert_eq!(content_type_header("B"), "multipart/form-data; boundary=B");
    }

    #[test]
    fn curl_flags_and_back() {
        let parts = parts_of(&rows(), &mut |s| s.to_string());
        let flags = curl_flags(&parts);
        assert_eq!(flags, ["title=Bando", "logo=@/tmp/logo.png;type=image/png"]);
        let back: Vec<KeyValue> = flags.iter().filter_map(|f| row_from_curl(f)).collect();
        assert_eq!((back[1].key.as_str(), back[1].value.as_str(), back[1].description.as_str()), ("logo", "@/tmp/logo.png", "image/png"));
        assert_eq!(row_from_curl("f=<note.txt").unwrap().value, "@note.txt");
        assert!(row_from_curl("=x").is_none());
    }
}
