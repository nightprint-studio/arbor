//! `validation` domain — the Struts-validation authoring handlers.
//!
//! * `bennu_validation_context` — given a `<Action>-validation.xml` path, the bound action class,
//!   its writable bean properties (the `<field name>` candidates) + fields already validated.
//! * `bennu_validation_author` — the write side: append an ordered validator **chain** to a field
//!   in a document string (pure — delegates to `bennu_web::prelude::append_validator`).
//! * `bennu_validation_target` — resolve the `<Class>-validation.xml` path bound to a Java action
//!   class (naming convention), whether it already exists, and the content to open (existing file
//!   or a fresh skeleton), for the "Create validation file" toolbar action.

use std::path::Path;

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::ValidationContext;
use bennu_web::prelude::{
    all_validators, append_validator, author_validation_skeleton, validation_file_for_class,
    AuthoredMessage, AuthoredValidator, ParamKind,
};
use serde::{Deserialize, Serialize};

use crate::index_service::IndexService;

/// Args for [`bennu_validation_context`].
#[derive(Deserialize)]
pub struct ValidationContextArgs {
    /// Absolute path (forward slashes) to the `<Action>-validation.xml` being edited.
    pub file: String,
}

/// Resolve the modal context for a validation file. `[]`/`None` fields when the action
/// class isn't indexed yet (the modal degrades to a free-text field name).
#[arbor_rpc::handler]
fn bennu_validation_context(
    _ctx: &BennuState,
    args: ValidationContextArgs,
) -> Result<ValidationContext, String> {
    Ok(IndexService::global().validation_context(&args.file))
}

// ── Authoring ──────────────────────────────────────────────────────────────────

/// Wire shape of a validator to author (the FE `ValidatorChainItem`).
#[derive(Deserialize)]
pub struct AuthoredValidatorWire {
    pub type_name: String,
    #[serde(default)]
    pub params: Vec<ParamWire>,
    pub message: Option<MessageWire>,
    #[serde(default)]
    pub short_circuit: bool,
}

#[derive(Deserialize)]
pub struct ParamWire {
    pub name: String,
    pub value: String,
}

#[derive(Deserialize)]
pub struct MessageWire {
    pub key: Option<String>,
    #[serde(default)]
    pub text: String,
}

/// Map a wire validator to the bennu-web authoring shape. Pure — unit tested.
fn to_authored(w: AuthoredValidatorWire) -> AuthoredValidator {
    AuthoredValidator {
        type_name: w.type_name,
        params: w.params.into_iter().map(|p| (p.name, p.value)).collect(),
        message: w.message.map(|m| AuthoredMessage { key: m.key, text: m.text }),
        short_circuit: w.short_circuit,
    }
}

/// Args for [`bennu_validation_author`].
#[derive(Deserialize)]
pub struct ValidationAuthorArgs {
    /// The current document text (the open buffer, or `""` for a fresh file).
    pub existing_xml: String,
    /// The field (action property) to add the chain to.
    pub field: String,
    /// The ordered validator chain to append.
    pub validators: Vec<AuthoredValidatorWire>,
}

/// Append the validator chain to `field` in `existing_xml`, returning the new full document.
/// Pure passthrough to `append_validator` (no filesystem — the FE writes the result).
#[arbor_rpc::handler]
fn bennu_validation_author(_ctx: &BennuState, args: ValidationAuthorArgs) -> Result<String, String> {
    let validators: Vec<AuthoredValidator> = args.validators.into_iter().map(to_authored).collect();
    Ok(append_validator(&args.existing_xml, &args.field, &validators))
}

/// Args for [`bennu_validation_target`].
#[derive(Deserialize)]
pub struct ValidationTargetArgs {
    /// The Java action-class file the user is editing.
    pub file: String,
}

/// The `<Class>-validation.xml` bound to a Java action class + whether it exists + the content to
/// open (existing file text, or a fresh DTD-headed skeleton to write).
#[derive(Serialize)]
pub struct ValidationTargetResult {
    /// Absolute path (forward slashes) of the validation file.
    pub path: String,
    /// True when the file already exists on disk.
    pub exists: bool,
    /// Content: the existing file text, or a fresh skeleton when it doesn't exist yet.
    pub content: String,
}

/// Resolve the validation file for the Java action class `file`. `None` when `file` isn't a
/// `.java` path. Reads the existing file (best effort) or emits a skeleton for a new one.
#[arbor_rpc::handler]
fn bennu_validation_target(
    _ctx: &BennuState,
    args: ValidationTargetArgs,
) -> Result<Option<ValidationTargetResult>, String> {
    let Some(path) = validation_file_for_class(Path::new(&args.file)) else {
        return Ok(None);
    };
    let exists = path.exists();
    let content = if exists {
        std::fs::read_to_string(&path).unwrap_or_else(|_| author_validation_skeleton())
    } else {
        author_validation_skeleton()
    };
    Ok(Some(ValidationTargetResult {
        path: path.to_string_lossy().replace('\\', "/"),
        exists,
        content,
    }))
}

// ── Validator vocabulary (shared with the FE chain-builder) ─────────────────────

/// Wire shape of a validator definition (the catalog row).
#[derive(Serialize)]
pub struct ValidatorDefWire {
    pub type_name: String,
    pub label: String,
    pub is_field: bool,
    pub params: Vec<ParamDefWire>,
}

/// Wire shape of one validator param (name + value kind + required).
#[derive(Serialize)]
pub struct ParamDefWire {
    pub name: String,
    /// One of `bool | int | long | double | date | text | ognl | regex` — drives the FE control.
    pub kind: String,
    pub required: bool,
}

fn kind_str(k: ParamKind) -> &'static str {
    match k {
        ParamKind::Bool => "bool",
        ParamKind::Int => "int",
        ParamKind::Long => "long",
        ParamKind::Double => "double",
        ParamKind::Date => "date",
        ParamKind::Text => "text",
        ParamKind::Ognl => "ognl",
        ParamKind::Regex => "regex",
    }
}

/// The built-in Struts2 validator vocabulary — so the FE chain-builder renders the validator
/// picker + per-type param inputs from the same source of truth as the authoring layer.
#[arbor_rpc::handler]
fn bennu_validator_catalog(_ctx: &BennuState) -> Result<Vec<ValidatorDefWire>, String> {
    Ok(all_validators()
        .iter()
        .map(|v| ValidatorDefWire {
            type_name: v.type_name.to_string(),
            label: v.label.to_string(),
            is_field: v.is_field,
            params: v
                .params
                .iter()
                .map(|p| ParamDefWire {
                    name: p.name.to_string(),
                    kind: kind_str(p.kind).to_string(),
                    required: p.required,
                })
                .collect(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_maps_to_authored_validator() {
        let w = AuthoredValidatorWire {
            type_name: "stringlength".into(),
            params: vec![ParamWire { name: "maxLength".into(), value: "10".into() }],
            message: Some(MessageWire { key: Some("k".into()), text: "too long".into() }),
            short_circuit: true,
        };
        let a = to_authored(w);
        assert_eq!(a.type_name, "stringlength");
        assert_eq!(a.params, vec![("maxLength".to_string(), "10".to_string())]);
        assert!(a.short_circuit);
        assert_eq!(a.message.unwrap().text, "too long");
    }
}
