//! Turning the bytes of a response into text, the way a browser would: by the charset the server declared,
//! else by what the document says about itself (BOM, `<meta charset>`, an XML declaration), else UTF-8, else
//! Windows-1252. Only what is really binary (an image, a PDF, an archive) is left as bytes.

use encoding_rs::{Encoding, UTF_8, WINDOWS_1252};

/// What a body turned out to be.
#[derive(Debug, PartialEq, Eq)]
pub struct Decoded {
    pub text: String,
    /// The bytes are not text: the caller keeps them (for a preview or a save) and shows `text` only as a fallback.
    pub binary: bool,
}

/// Media types whose body is text whatever the charset is.
pub fn is_textual(content_type: &str) -> bool {
    let media = content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    media.starts_with("text/")
        || media.ends_with("+json")
        || media.ends_with("+xml")
        || matches!(
            media.as_str(),
            "application/json" | "application/xml" | "application/javascript" | "application/ecmascript" | "application/x-javascript"
                | "application/x-www-form-urlencoded" | "application/graphql" | "application/yaml" | "application/x-yaml" | "application/x-ndjson"
        )
}

/// The `charset=` of a `Content-Type`, when it names an encoding that is known.
pub fn declared_charset(content_type: &str) -> Option<&'static Encoding> {
    let lower = content_type.to_ascii_lowercase();
    let at = lower.find("charset=")?;
    let label = lower[at + 8..].split(';').next()?.trim().trim_matches(|c| c == '"' || c == '\'');
    Encoding::for_label(label.as_bytes())
}

/// What the document says about itself in its first bytes: `<meta charset=…>`, `<meta http-equiv … charset=…>`, `<?xml … encoding="…"?>`.
fn sniff_charset(bytes: &[u8]) -> Option<&'static Encoding> {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(2048)]).to_ascii_lowercase();
    for key in ["charset=", "encoding="] {
        let mut from = 0;
        while let Some(at) = head[from..].find(key) {
            let start = from + at + key.len();
            let label: String = head[start..].trim_start_matches(|c| c == '"' || c == '\'').chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')).collect();
            if let Some(enc) = Encoding::for_label(label.as_bytes()) {
                return Some(enc);
            }
            from = start;
        }
    }
    None
}

/// Decode `bytes`, given the response's `Content-Type` (empty when there is none).
pub fn decode(bytes: &[u8], content_type: &str) -> Decoded {
    if let Some((enc, skip)) = Encoding::for_bom(bytes) {
        let (text, _) = enc.decode_without_bom_handling(&bytes[skip..]);
        return Decoded { text: text.into_owned(), binary: false };
    }
    let textual = is_textual(content_type);
    if !textual && !valid_text(bytes) {
        return Decoded { text: String::from_utf8_lossy(bytes).into_owned(), binary: true };
    }
    let encoding = declared_charset(content_type)
        .or_else(|| sniff_charset(bytes))
        .unwrap_or_else(|| if std::str::from_utf8(bytes).is_ok() { UTF_8 } else { WINDOWS_1252 });
    let (text, _) = encoding.decode_without_bom_handling(bytes);
    Decoded { text: text.into_owned(), binary: false }
}

/// A type the server did not call text can still be: valid UTF-8 without a NUL is.
fn valid_text(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok() && !bytes.contains(&0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_in_latin1_is_decoded_by_the_declared_charset() {
        let bytes = b"<html><body>caff\xe8 \xe0 la carte</body></html>";
        let d = decode(bytes, "text/html; charset=ISO-8859-1");
        assert!(!d.binary);
        assert!(d.text.contains("caffè à la carte"));
    }

    #[test]
    fn a_meta_charset_is_followed_when_the_header_says_nothing() {
        let bytes = b"<html><head><meta charset=\"windows-1252\"></head><body>\x93quoted\x94</body></html>";
        let d = decode(bytes, "text/html");
        assert!(d.text.contains('\u{201c}'), "{:?}", d.text);
    }

    #[test]
    fn an_xml_declaration_is_followed() {
        let bytes = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a>\xe9</a>";
        assert!(decode(bytes, "application/xml").text.contains('é'));
    }

    #[test]
    fn utf8_json_and_plus_json_are_text() {
        assert_eq!(decode("{\"a\":\"è\"}".as_bytes(), "application/json"), Decoded { text: "{\"a\":\"è\"}".into(), binary: false });
        assert!(!decode(b"{}", "application/problem+json").binary);
    }

    #[test]
    fn a_text_type_without_charset_falls_back_to_windows_1252_when_it_is_not_utf8() {
        assert_eq!(decode(b"caff\xe8", "text/plain").text, "caffè");
    }

    #[test]
    fn a_bom_wins() {
        let bytes = [0xEF, 0xBB, 0xBF, b'o', b'k'];
        assert_eq!(decode(&bytes, "text/plain; charset=iso-8859-1").text, "ok");
    }

    #[test]
    fn real_binaries_stay_binary() {
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0xFF, 0x00];
        assert!(decode(&png, "image/png").binary);
        assert!(decode(&[0xC3, 0x28, 0x00, 0x01], "application/octet-stream").binary);
    }

    #[test]
    fn an_unlabelled_utf8_body_is_text() {
        assert!(!decode("ciao è".as_bytes(), "").binary);
        assert!(!decode("ciao è".as_bytes(), "application/x-custom").binary);
    }
}
