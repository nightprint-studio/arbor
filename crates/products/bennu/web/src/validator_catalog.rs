//! The Struts2 **validator vocabulary** — a static registry of the built-in validators and the
//! `<param>`s each accepts, shared by the authoring layer and (later) FE autocomplete/inlay hints.
//!
//! One source of truth so the chain-builder UI, the parser's understanding and any future
//! diagnostic ("unknown validator", "missing required param") all agree. Only the common built-in
//! set is modelled (the 99% of legacy rulesets); a project can still author an unknown type — the
//! catalog is advisory, not a gate.

/// The value shape a `<param>` carries — drives the FE input control (checkbox for `Bool`, number
/// for `Int`/`Double`, an OGNL/regex text field, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    Bool,
    Int,
    Long,
    Double,
    Date,
    Text,
    /// An OGNL expression (`fieldexpression` / `expression`).
    Ognl,
    /// A regular expression (`regex` validator).
    Regex,
}

/// One `<param>` a validator accepts.
#[derive(Debug, Clone, Copy)]
pub struct ParamDef {
    pub name: &'static str,
    pub kind: ParamKind,
    /// True when Struts requires the param for the validator to do anything meaningful.
    pub required: bool,
}

/// A built-in validator: its `type`, whether it's a **field** validator (`<field-validator>`) or a
/// non-field `<validator>` (only `expression`), a UI label, and its params.
#[derive(Debug, Clone, Copy)]
pub struct ValidatorDef {
    pub type_name: &'static str,
    pub label: &'static str,
    /// `true` → `<field-validator>` (bound to a field); `false` → top-level `<validator>`.
    pub is_field: bool,
    pub params: &'static [ParamDef],
}

const fn p(name: &'static str, kind: ParamKind, required: bool) -> ParamDef {
    ParamDef { name, kind, required }
}

const REQUIRED: &[ParamDef] = &[];
const REQUIREDSTRING: &[ParamDef] = &[p("trim", ParamKind::Bool, false)];
const STRINGLENGTH: &[ParamDef] = &[
    p("minLength", ParamKind::Int, false),
    p("maxLength", ParamKind::Int, false),
    p("trim", ParamKind::Bool, false),
];
const INT: &[ParamDef] = &[p("min", ParamKind::Int, false), p("max", ParamKind::Int, false)];
const LONG: &[ParamDef] = &[p("min", ParamKind::Long, false), p("max", ParamKind::Long, false)];
const DOUBLE: &[ParamDef] = &[
    p("minInclusive", ParamKind::Double, false),
    p("maxInclusive", ParamKind::Double, false),
    p("minExclusive", ParamKind::Double, false),
    p("maxExclusive", ParamKind::Double, false),
];
const DATE: &[ParamDef] =
    &[p("min", ParamKind::Date, false), p("max", ParamKind::Date, false)];
const EMPTY: &[ParamDef] = &[];
const REGEX: &[ParamDef] = &[
    p("regexExpression", ParamKind::Regex, true),
    p("caseSensitive", ParamKind::Bool, false),
    p("trim", ParamKind::Bool, false),
];
const FIELDEXPRESSION: &[ParamDef] = &[p("expression", ParamKind::Ognl, true)];
const EXPRESSION: &[ParamDef] = &[p("expression", ParamKind::Ognl, true)];
const CONVERSION: &[ParamDef] = &[p("repopulateField", ParamKind::Bool, false)];

/// The built-in validator registry, in a sensible UI order (most-common first).
static VALIDATORS: &[ValidatorDef] = &[
    ValidatorDef { type_name: "required", label: "Required (not null)", is_field: true, params: REQUIRED },
    ValidatorDef { type_name: "requiredstring", label: "Required string", is_field: true, params: REQUIREDSTRING },
    ValidatorDef { type_name: "stringlength", label: "String length", is_field: true, params: STRINGLENGTH },
    ValidatorDef { type_name: "int", label: "Integer range", is_field: true, params: INT },
    ValidatorDef { type_name: "long", label: "Long range", is_field: true, params: LONG },
    ValidatorDef { type_name: "double", label: "Double range", is_field: true, params: DOUBLE },
    ValidatorDef { type_name: "date", label: "Date range", is_field: true, params: DATE },
    ValidatorDef { type_name: "email", label: "Email", is_field: true, params: EMPTY },
    ValidatorDef { type_name: "url", label: "URL", is_field: true, params: EMPTY },
    ValidatorDef { type_name: "regex", label: "Regular expression", is_field: true, params: REGEX },
    ValidatorDef { type_name: "fieldexpression", label: "Field expression (OGNL)", is_field: true, params: FIELDEXPRESSION },
    ValidatorDef { type_name: "expression", label: "Expression (OGNL, non-field)", is_field: false, params: EXPRESSION },
    ValidatorDef { type_name: "conversion", label: "Conversion error", is_field: true, params: CONVERSION },
];

/// Every built-in validator, in UI order.
pub fn all_validators() -> &'static [ValidatorDef] {
    VALIDATORS
}

/// Look up a validator by its `type` name (`None` for an unknown/custom type).
pub fn validator_def(type_name: &str) -> Option<&'static ValidatorDef> {
    VALIDATORS.iter().find(|v| v.type_name == type_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_validators_resolve() {
        assert!(validator_def("required").is_some());
        assert!(validator_def("regex").is_some());
        assert!(validator_def("does-not-exist").is_none());
    }

    #[test]
    fn expression_is_the_only_non_field_validator() {
        for v in all_validators() {
            if v.type_name == "expression" {
                assert!(!v.is_field, "expression must be a non-field validator");
            } else {
                assert!(v.is_field, "{} should be a field validator", v.type_name);
            }
        }
    }

    #[test]
    fn regex_requires_its_expression_param() {
        let def = validator_def("regex").unwrap();
        let re = def.params.iter().find(|p| p.name == "regexExpression").unwrap();
        assert!(re.required);
        assert_eq!(re.kind, ParamKind::Regex);
    }

    #[test]
    fn stringlength_params_are_optional_ints_and_a_trim_bool() {
        let def = validator_def("stringlength").unwrap();
        assert_eq!(def.params.len(), 3);
        assert!(def.params.iter().all(|p| !p.required));
        assert_eq!(def.params.iter().find(|p| p.name == "minLength").unwrap().kind, ParamKind::Int);
        assert_eq!(def.params.iter().find(|p| p.name == "trim").unwrap().kind, ParamKind::Bool);
    }

    #[test]
    fn required_has_no_params() {
        assert!(validator_def("required").unwrap().params.is_empty());
    }

    #[test]
    fn registry_is_non_empty_and_unique() {
        assert!(all_validators().len() >= 12);
        let mut seen = std::collections::HashSet::new();
        for v in all_validators() {
            assert!(seen.insert(v.type_name), "duplicate validator {}", v.type_name);
        }
    }
}
