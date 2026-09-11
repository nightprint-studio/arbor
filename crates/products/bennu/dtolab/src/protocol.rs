//! The wire between the backend and the Java harness — and the harness itself.
//!
//! ## Why a line protocol with base64 arguments
//!
//! The harness runs on whatever JDK the project uses, with nothing on its classpath but itself: it
//! cannot assume a JSON library, because the project's own Jackson — if it has one — lives in a
//! class loader the harness creates later. So a request is one line of tab-separated fields, every
//! argument base64-encoded so a payload may hold tabs and newlines; and a reply is one line holding
//! the request id and a JSON body the harness writes by hand, which is the easy direction.
//!
//! Every reply body has `ok`. `ok: false` carries `error`: the harness understood the request and
//! the answer is a failure — a class that does not load, a validator that is not on the classpath.
//! That is a *result*, and the session that produced it is still good.

use std::collections::BTreeMap;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};

/// The harness's class name, which is also its file name.
pub const HARNESS_CLASS: &str = "BennuLabHarness";

/// The harness, compiled on first use with the project's own `javac` and cached per JDK.
pub const HARNESS_SOURCE: &str = include_str!("../harness/BennuLabHarness.java");

/// One request line, newline included.
pub fn encode_request(id: u64, op: &str, args: &[&str]) -> String {
    let mut line = format!("{id}\t{op}");
    for arg in args {
        line.push('\t');
        line.push_str(&STANDARD.encode(arg.as_bytes()));
    }
    line.push('\n');
    line
}

/// A reply line as `(id, body)`. `None` for anything that is not one.
pub fn decode_reply(line: &str) -> Option<(u64, serde_json::Value)> {
    let (id, body) = line.trim_end_matches(['\r', '\n']).split_once('\t')?;
    Some((id.parse().ok()?, serde_json::from_str(body).ok()?))
}

/// One constraint violation, as the project's validator reported it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    /// The property path (`customerName`, `lines[0].quantity`).
    pub path: String,
    /// The message template before interpolation (`{jakarta.validation.constraints.Size.message}`).
    pub template: String,
    /// The message the user would see, interpolated in the locale asked for.
    pub message: String,
    /// The constraint annotation, fully qualified.
    pub constraint: String,
    /// The constraint's own attributes, without `message`, `groups` and `payload`.
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
    /// The rejected value, as text.
    #[serde(default)]
    pub invalid_value: Option<String>,
    /// The constant holding the template or the message, when a template asked for constants to be
    /// resolved — see [`crate::template::declared_constant_classes`].
    #[serde(default)]
    pub constant: Option<String>,
}

/// A class as the validator itself sees it: every constrained property with its constraints —
/// including composed and custom ones, which no reading of the source can recognise.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Described {
    #[serde(default)]
    pub properties: Vec<DescribedProperty>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescribedProperty {
    pub name: String,
    /// The element type's simple name (`String`, `List`, `int[]`).
    pub type_name: String,
    #[serde(default)]
    pub constraints: Vec<DescribedConstraint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescribedConstraint {
    /// Fully-qualified annotation type.
    pub annotation: String,
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
    #[serde(default)]
    pub template: String,
}

/// The violations in a `validate` reply body. Empty when there are none, or none could be read.
pub fn violations_of(body: &serde_json::Value) -> Vec<Violation> {
    body.get("violations")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A payload is free text: tabs and newlines in it must not split the request line.
    #[test]
    fn an_argument_with_tabs_and_newlines_stays_one_field() {
        let line = encode_request(7, "validate", &["com.example.Order", "{\n\t\"a\": 1\n}"]);
        assert!(line.ends_with('\n'));
        let fields: Vec<&str> = line.trim_end().split('\t').collect();
        assert_eq!(fields.len(), 4);
        assert_eq!(fields[0], "7");
        assert_eq!(fields[1], "validate");
        let payload = STANDARD.decode(fields[3]).unwrap();
        assert_eq!(String::from_utf8(payload).unwrap(), "{\n\t\"a\": 1\n}");
    }

    #[test]
    fn a_reply_is_read_back_with_its_violations() {
        let (id, body) = decode_reply(
            "3\t{\"ok\":true,\"violations\":[{\"path\":\"name\",\"template\":\"{t}\",\"message\":\"m\",\"constraint\":\"jakarta.validation.constraints.NotBlank\",\"attributes\":{}}]}\r\n",
        )
        .unwrap();
        assert_eq!(id, 3);
        let found = violations_of(&body);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, "name");
        assert_eq!(found[0].constant, None);
    }

    #[test]
    fn a_line_that_is_not_a_reply_is_ignored() {
        assert!(decode_reply("Picked up JAVA_TOOL_OPTIONS: -Xmx1g").is_none());
    }
}
