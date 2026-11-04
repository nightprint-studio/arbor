//! HTTP methods.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    #[default]
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl Method {
    pub const ALL: [Method; 7] = [Method::Get, Method::Post, Method::Put, Method::Patch, Method::Delete, Method::Head, Method::Options];

    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Head => "HEAD",
            Method::Options => "OPTIONS",
        }
    }

    pub fn parse(word: &str) -> Option<Method> {
        let w = word.trim().to_ascii_uppercase();
        Method::ALL.into_iter().find(|m| m.as_str() == w)
    }

    /// Whether the method changes something on the server. The ones a protected environment asks about.
    pub fn is_mutating(self) -> bool {
        matches!(self, Method::Post | Method::Put | Method::Patch | Method::Delete)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methods_are_read_in_any_case_and_only_the_changing_ones_mutate() {
        assert_eq!(Method::parse(" post "), Some(Method::Post));
        assert_eq!(Method::parse("delete"), Some(Method::Delete));
        assert_eq!(Method::parse("FETCH"), None);
        let mutating: Vec<_> = Method::ALL.into_iter().filter(|m| m.is_mutating()).map(Method::as_str).collect();
        assert_eq!(mutating, ["POST", "PUT", "PATCH", "DELETE"]);
    }

    #[test]
    fn a_method_is_written_in_capitals() {
        assert_eq!(serde_json::to_string(&Method::Patch).unwrap(), "\"PATCH\"");
        assert_eq!(serde_json::from_str::<Method>("\"GET\"").unwrap(), Method::Get);
    }
}
