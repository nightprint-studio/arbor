//! Permission schemas, requirement predicates, and the manifest validation
//! surface (decision **D2** in the architecture doc: typed core + free-form
//! `ext` for crate-contributed keys).
//!
//! The split is:
//!
//! - [`PermSchema`] describes the *shape* of a permission's value (a bool,
//!   a string, a list of strings, an enum with a known low → high ordering).
//! - [`PermReq`] is a *predicate* a plugin function attaches to its
//!   declaration — "needs `gitprovider` at least `read`", etc.
//! - [`PermissionDef`] glues them together with a key, default, and human
//!   description for the docs panel.
//!
//! Cross-runtime: schemas live in `&'static` data baked into each
//! contributor; the actual values come from `plugin.toml` and are exposed
//! through the [`PermissionsView`] trait the registry materialises at
//! invocation time.

use crate::error::PluginError;

// ── Schemas ────────────────────────────────────────────────────────────────

/// Shape of a permission's value as declared in `plugin.toml`.
#[derive(Debug, Clone)]
pub enum PermSchema {
    /// `key = true` / `key = false`.
    Bool,
    /// `key = "value"`. Any string accepted.
    String,
    /// `key = ["a", "b"]`.
    StringList,
    /// `key = "value"` where `value` must be one of the listed options.
    ///
    /// **Ordered low → high** so that [`PermReq::AtLeast`] can compare
    /// values by index. E.g. `&["none", "read", "write"]` — "write" satisfies
    /// `AtLeast("…", "read")` because `2 >= 1`.
    Enum(&'static [&'static str]),
}

impl PermSchema {
    /// Human-readable name used in error messages.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Bool => "boolean",
            Self::String => "string",
            Self::StringList => "string[]",
            Self::Enum(_) => "enum",
        }
    }

    /// Validate that a TOML value matches the schema.
    ///
    /// Returns `Ok(())` on a match, or a short human-readable explanation on
    /// mismatch — surfaced through [`ManifestPermError`] by the registry.
    pub fn validate(&self, value: &toml::Value) -> Result<(), String> {
        match (self, value) {
            (Self::Bool, toml::Value::Boolean(_)) => Ok(()),
            (Self::String, toml::Value::String(_)) => Ok(()),
            (Self::StringList, toml::Value::Array(items)) => {
                for (i, v) in items.iter().enumerate() {
                    if !v.is_str() {
                        return Err(format!("item #{i} must be a string"));
                    }
                }
                Ok(())
            }
            (Self::Enum(options), toml::Value::String(s)) => {
                if options.contains(&s.as_str()) {
                    Ok(())
                } else {
                    Err(format!("must be one of: {}", options.join(", ")))
                }
            }
            (schema, _) => Err(format!(
                "type mismatch: expected {}",
                schema.kind_name()
            )),
        }
    }
}

// ── Requirement predicates ─────────────────────────────────────────────────

/// A single permission predicate attached to a plugin-facing function.
///
/// Multiple requirements are AND-combined by the registry: every entry must
/// hold for the call to proceed.
#[derive(Debug, Clone, Copy)]
pub enum PermReq {
    /// The plugin must declare the permission at all (any non-empty / non-`false`
    /// value).
    Has(&'static str),
    /// The plugin's value for the permission must be at least as permissive
    /// as the listed option — only meaningful against an [`PermSchema::Enum`].
    AtLeast(&'static str, &'static str),
    /// The plugin's value must exactly equal the listed value.
    Equals(&'static str, &'static str),
}

impl PermReq {
    /// The permission key this requirement targets.
    pub fn key(&self) -> &'static str {
        match self {
            Self::Has(k) | Self::AtLeast(k, _) | Self::Equals(k, _) => k,
        }
    }

    /// Run the predicate against a [`PermissionsView`].
    ///
    /// Returns [`PluginError::PermissionDenied`] when the plugin does not
    /// satisfy the requirement. The error's second field carries the required
    /// value verbatim (`"read"`, `"write"`, `"set"`, …) so it can be shown
    /// directly to the user.
    pub fn check<P: PermissionsView + ?Sized>(&self, perms: &P) -> Result<(), PluginError> {
        match self {
            Self::Has(key) => {
                let truthy = perms.get(key).map(|v| match v {
                    toml::Value::Boolean(b) => *b,
                    toml::Value::String(s) => !s.is_empty(),
                    toml::Value::Array(a) => !a.is_empty(),
                    _ => true,
                });
                if matches!(truthy, Some(true)) {
                    Ok(())
                } else {
                    Err(PluginError::PermissionDenied((*key).into(), "set".into()))
                }
            }
            Self::AtLeast(key, required) => {
                if perms.at_least(key, required) {
                    Ok(())
                } else {
                    Err(PluginError::PermissionDenied((*key).into(), (*required).into()))
                }
            }
            Self::Equals(key, required) => {
                let ok = matches!(
                    perms.get(key),
                    Some(toml::Value::String(s)) if s == required
                );
                if ok {
                    Ok(())
                } else {
                    Err(PluginError::PermissionDenied((*key).into(), (*required).into()))
                }
            }
        }
    }
}

// ── Definition (the unit a crate contributes) ──────────────────────────────

/// A permission key declared by a domain crate.
///
/// Lifetime story: keys, descriptions, and enum options are `&'static` because
/// they're baked into the contributor's source. The `default` is a `toml::Value`
/// (not static) so that custom defaults — e.g. a list — are easy to spell
/// without making the whole API generic over a value lifetime.
#[derive(Debug, Clone)]
pub struct PermissionDef {
    pub key:         &'static str,
    pub schema:      PermSchema,
    pub default:     toml::Value,
    pub description: &'static str,
    /// Other permission keys this one implies / requires. Validated when a
    /// plugin's manifest is checked against the registry.
    pub requires:    &'static [PermReq],
}

// ── Materialised view (what the registry hands the gate) ───────────────────

/// Runtime view of a plugin's permission table, joined against the registry's
/// schemas.
///
/// The registry builds this on demand around each invocation: it combines the
/// plugin's `plugin.toml` permissions (read through [`crate::ctx::PluginCtx`])
/// with the schema metadata it owns. Domain crates do **not** implement this
/// trait directly — they declare [`PermissionDef`]s and the registry wires the
/// rest.
pub trait PermissionsView {
    /// Raw TOML value for a permission, or `None` if not declared by the
    /// plugin (and no default applies).
    fn get(&self, key: &str) -> Option<&toml::Value>;

    /// `true` if the plugin's value for an [`PermSchema::Enum`] permission is
    /// at least as permissive as `required`. Returns `false` for unknown keys,
    /// non-enum schemas, or values that don't appear in the enum.
    fn at_least(&self, key: &str, required: &str) -> bool;
}

// ── Manifest validation surface ────────────────────────────────────────────

/// Per-key failure produced by [`crate::registry::PluginRegistry::validate_manifest`].
#[derive(Debug, Clone)]
pub struct ManifestPermError {
    pub key:     String,
    pub message: String,
}

impl std::fmt::Display for ManifestPermError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.key, self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn tval_bool(b: bool) -> toml::Value {
        toml::Value::Boolean(b)
    }
    fn tval_str(s: &str) -> toml::Value {
        toml::Value::String(s.to_string())
    }

    struct StubView {
        values: HashMap<&'static str, toml::Value>,
        // pre-computed enum order, keyed by perm name.
        enums:  HashMap<&'static str, &'static [&'static str]>,
    }

    impl PermissionsView for StubView {
        fn get(&self, key: &str) -> Option<&toml::Value> {
            self.values.get(key)
        }
        fn at_least(&self, key: &str, required: &str) -> bool {
            let Some(options) = self.enums.get(key) else { return false };
            let Some(toml::Value::String(actual)) = self.values.get(key) else {
                return false;
            };
            let req_idx = options.iter().position(|o| *o == required);
            let cur_idx = options.iter().position(|o| *o == actual.as_str());
            matches!((req_idx, cur_idx), (Some(r), Some(c)) if c >= r)
        }
    }

    #[test]
    fn schema_validate_accepts_matching_shapes() {
        assert!(PermSchema::Bool.validate(&tval_bool(true)).is_ok());
        assert!(PermSchema::String.validate(&tval_str("x")).is_ok());
        assert!(
            PermSchema::Enum(&["none", "read", "write"])
                .validate(&tval_str("read"))
                .is_ok()
        );
    }

    #[test]
    fn schema_validate_rejects_unknown_enum_value() {
        let err = PermSchema::Enum(&["none", "read"])
            .validate(&tval_str("admin"))
            .unwrap_err();
        assert!(err.contains("read"));
    }

    #[test]
    fn perm_req_has_passes_for_truthy_value() {
        let view = StubView {
            values: [("fs", tval_bool(true))].into_iter().collect(),
            enums:  HashMap::new(),
        };
        assert!(PermReq::Has("fs").check(&view).is_ok());
    }

    #[test]
    fn perm_req_has_denies_for_falsy_value() {
        let view = StubView {
            values: [("fs", tval_bool(false))].into_iter().collect(),
            enums:  HashMap::new(),
        };
        let err = PermReq::Has("fs").check(&view).unwrap_err();
        assert!(matches!(err, PluginError::PermissionDenied(_, _)));
    }

    #[test]
    fn perm_req_at_least_uses_enum_ordering() {
        let view = StubView {
            values: [("gitprovider", tval_str("write"))].into_iter().collect(),
            enums:  [("gitprovider", &["none", "read", "write"][..])]
                .into_iter()
                .collect(),
        };
        assert!(
            PermReq::AtLeast("gitprovider", "read")
                .check(&view)
                .is_ok()
        );
        // And the reverse fails.
        let view_low = StubView {
            values: [("gitprovider", tval_str("read"))].into_iter().collect(),
            enums:  [("gitprovider", &["none", "read", "write"][..])]
                .into_iter()
                .collect(),
        };
        assert!(
            PermReq::AtLeast("gitprovider", "write")
                .check(&view_low)
                .is_err()
        );
    }

    #[test]
    fn perm_req_equals_is_exact_match() {
        let view = StubView {
            values: [("env_read", tval_str("HOME"))].into_iter().collect(),
            enums:  HashMap::new(),
        };
        assert!(PermReq::Equals("env_read", "HOME").check(&view).is_ok());
        assert!(PermReq::Equals("env_read", "PATH").check(&view).is_err());
    }
}
