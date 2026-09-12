//! The values a field is given for what it is called — `email`, `codiceFiscale`, `iban` — or for the
//! custom constraint it carries.
//!
//! A constraint says what a value must not be; it rarely says what a value *is*. `@Pattern` and every
//! custom constraint leave the valid instance with nothing to put in the field, and a valid instance
//! that is not valid makes every generated test expect its violation too. The name of a field very
//! often knows what the constraint does not, so a map from names to values fills exactly that gap — and
//! makes the rest of a test readable: `"mario.rossi@example.com"` rather than `"x".repeat(3)`.
//!
//! ## Matching
//!
//! Names are compared ignoring case and everything that is not a letter or a digit, so
//! `codiceFiscale`, `codice_fiscale` and `CODICE_FISCALE` are one name; a `*` at either end matches any
//! prefix or suffix (`*email` is `billingEmail` too). Constraints are matched on their simple name the
//! same way. A rule answering a constraint the field carries comes before one answering its name — a
//! custom constraint is the more reliable statement about what a field holds — and otherwise the first
//! rule wins, which is why the built-ins list the specific before the general (`emailAddress` is an
//! email, not an address).
//!
//! ## Values
//!
//! Fixed, never random: a generated test that changes on every generation is a diff nobody can review.
//! A rule may carry a Java expression to write instead of the literal — a team's own factory — and the
//! literal is still what the JVM checks.
//!
//! A rule's value must **fit** the field to be used: a number for a numeric field, and inside what can
//! be read here — `@Size`, `@Min` / `@Max`, `@Positive` and family, `@Email`. One that does not fit is
//! passed over, and the value computed from the constraints stands. What cannot be read here — a
//! `@Pattern`, a custom validator — the JVM check of the valid instance still reports.

use serde::{Deserialize, Serialize};

use bennu_templates::prelude::{type_simple, FieldModel};

use crate::cases::{parse_int, shape, SampleValue, Shape};

/// One rule: which fields it answers — by name, or by a constraint they carry — and what it gives them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueRule {
    /// Unique among a user's rules, and what a template reads as `value.name`: `email`, `codice_fiscale`.
    pub name: String,
    /// Field names, matched ignoring case, `_` and `-`; `*` at either end for any prefix or suffix.
    #[serde(default)]
    pub fields: Vec<String>,
    /// Constraint simple names, without `@`, matched the same way.
    #[serde(default)]
    pub constraints: Vec<String>,
    /// A valid value, as text — read as a number or a boolean for a field of that type.
    pub value: String,
    /// A Java expression written into a test instead of `value`: a team's own factory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java: Option<String>,
    /// A value the constraints this rule names reject — the invalid case of a constraint Bennu does not
    /// know, which otherwise has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invalid: Option<String>,
    /// A Java expression written instead of `invalid`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invalid_java: Option<String>,
}

struct Builtin {
    name: &'static str,
    fields: &'static [&'static str],
    constraints: &'static [&'static str],
    value: &'static str,
    invalid: Option<&'static str>,
}

/// The common Italian and English fields. Order matters: the specific before the general.
///
/// Every value is valid for what it names — the tax code, VAT number, IBAN and card number carry the
/// right check digits — and every invalid one fails exactly that check, so a validator that computes it
/// rejects it for the reason the test is about.
const BUILTINS: &[Builtin] = &[
    Builtin { name: "email", fields: &["email", "*email", "email*", "mail", "pec", "indirizzopec", "pecaddress", "postaelettronica", "postaelettronicacertificata"], constraints: &[], value: "mario.rossi@example.com", invalid: None },
    Builtin { name: "phone", fields: &["phone", "*phone", "phonenumber", "telephone", "tel", "telefono", "*telefono", "telefono*", "cellulare", "cell", "mobile", "mobilenumber"], constraints: &["Phone", "PhoneNumber", "Telefono"], value: "+393331234567", invalid: Some("not-a-phone") },
    Builtin { name: "fax", fields: &["fax", "*fax", "faxnumber"], constraints: &["Fax"], value: "+390212345678", invalid: Some("not-a-fax") },
    Builtin { name: "codice_fiscale", fields: &["codicefiscale", "*codicefiscale", "codfiscale", "codfisc", "cf", "fiscalcode"], constraints: &["CodiceFiscale", "CF", "FiscalCode"], value: "RSSMRA80A01H501U", invalid: Some("RSSMRA80A01H501X") },
    Builtin { name: "partita_iva", fields: &["partitaiva", "*partitaiva", "piva", "vat", "vatnumber", "*vatnumber", "vatcode", "vatid"], constraints: &["PartitaIva", "Piva", "VatNumber", "Vat"], value: "12345678903", invalid: Some("12345678901") },
    Builtin { name: "iban", fields: &["iban", "*iban"], constraints: &["Iban"], value: "IT60X0542811101000000123456", invalid: Some("IT00X0542811101000000123456") },
    Builtin { name: "bic", fields: &["bic", "swift", "swiftcode", "biccode", "codicebic", "codiceswift"], constraints: &["Bic", "Swift"], value: "BCITITMM", invalid: Some("BCIT") },
    Builtin { name: "card_number", fields: &["creditcard", "creditcardnumber", "cardnumber", "numerocarta", "pan"], constraints: &["CreditCardNumber", "LuhnCheck"], value: "4111111111111111", invalid: Some("4111111111111112") },
    Builtin { name: "postcode", fields: &["cap", "codicepostale", "zip", "zipcode", "*zipcode", "postcode", "postalcode", "*postalcode"], constraints: &["Cap", "ZipCode", "PostalCode"], value: "00184", invalid: Some("ABCDE") },
    Builtin { name: "url", fields: &["url", "*url", "website", "sitoweb", "sitointernet", "homepage", "link"], constraints: &["URL"], value: "https://www.example.com", invalid: Some("not a url") },
    Builtin { name: "uuid", fields: &["uuid", "*uuid", "guid"], constraints: &["UUID"], value: "123e4567-e89b-12d3-a456-426614174000", invalid: Some("not-a-uuid") },
    Builtin { name: "ip", fields: &["ip", "ipaddress", "indirizzoip"], constraints: &["IpAddress", "Ip"], value: "192.168.1.10", invalid: Some("999.1.1.1") },
    Builtin { name: "isbn", fields: &["isbn"], constraints: &["ISBN"], value: "978-3-16-148410-0", invalid: Some("978-3-16-148410-1") },
    Builtin { name: "username", fields: &["username", "login", "nomeutente"], constraints: &[], value: "mario.rossi", invalid: None },
    Builtin { name: "password", fields: &["password", "*password", "pwd", "passwd"], constraints: &[], value: "Str0ng!Passw0rd", invalid: None },
    Builtin { name: "full_name", fields: &["fullname", "nomecompleto", "nominativo", "nomecognome"], constraints: &[], value: "Mario Rossi", invalid: None },
    Builtin { name: "first_name", fields: &["firstname", "givenname", "name", "nome"], constraints: &[], value: "Mario", invalid: None },
    Builtin { name: "last_name", fields: &["lastname", "surname", "familyname", "cognome"], constraints: &[], value: "Rossi", invalid: None },
    Builtin { name: "company", fields: &["ragionesociale", "denominazione", "company", "companyname", "businessname", "azienda"], constraints: &[], value: "Acme S.r.l.", invalid: None },
    Builtin { name: "address", fields: &["address", "streetaddress", "indirizzo", "indirizzo*", "via"], constraints: &[], value: "Via Roma 1", invalid: None },
    Builtin { name: "city", fields: &["city", "town", "citta", "*citta", "città", "*città", "comune", "localita", "località"], constraints: &[], value: "Roma", invalid: None },
    Builtin { name: "province", fields: &["provincia", "siglaprovincia", "province"], constraints: &[], value: "RM", invalid: None },
    Builtin { name: "country", fields: &["country", "countrycode", "nazione", "paese"], constraints: &[], value: "IT", invalid: None },
    Builtin { name: "currency", fields: &["currency", "currencycode", "valuta"], constraints: &[], value: "EUR", invalid: None },
    Builtin { name: "language", fields: &["language", "languagecode", "lingua", "locale"], constraints: &[], value: "it", invalid: None },
    Builtin { name: "gender", fields: &["gender", "sex", "sesso"], constraints: &[], value: "M", invalid: None },
    Builtin { name: "age", fields: &["age", "eta", "età"], constraints: &[], value: "30", invalid: None },
    Builtin { name: "number_plate", fields: &["targa", "licenseplate", "plate", "platenumber"], constraints: &[], value: "AB123CD", invalid: None },
];

/// The rules Bennu ships, in the order they are tried.
pub fn builtin_rules() -> Vec<ValueRule> {
    BUILTINS
        .iter()
        .map(|b| ValueRule {
            name: b.name.to_string(),
            fields: b.fields.iter().map(|s| s.to_string()).collect(),
            constraints: b.constraints.iter().map(|s| s.to_string()).collect(),
            value: b.value.to_string(),
            java: None,
            invalid: b.invalid.map(str::to_string),
            invalid_java: None,
        })
        .collect()
}

/// The rules in force: `mine` first, in their order, then every built-in that is not switched off and
/// not replaced by one of `mine` with its name.
pub fn effective_rules(mine: &[ValueRule], disabled: &[String]) -> Vec<ValueRule> {
    let mut out = mine.to_vec();
    out.extend(
        builtin_rules()
            .into_iter()
            .filter(|b| !disabled.contains(&b.name) && !mine.iter().any(|m| m.name == b.name)),
    );
    out
}

/// Whether a user's rules can be kept: named, each name once, each matching something, each with a value.
pub fn check_rules(rules: &[ValueRule]) -> Result<(), String> {
    for (i, rule) in rules.iter().enumerate() {
        if rule.name.is_empty() || !rule.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!("`{}` is not a name for a value: letters, digits and `_`", rule.name));
        }
        if rules[..i].iter().any(|r| r.name == rule.name) {
            return Err(format!("Two values are called `{}`", rule.name));
        }
        if !rule.fields.iter().chain(&rule.constraints).any(|p| !normalize(p.trim_matches('*')).is_empty()) {
            return Err(format!("`{}` matches no field: give it a field name or a constraint", rule.name));
        }
        if rule.value.is_empty() {
            return Err(format!("`{}` has no value", rule.name));
        }
    }
    Ok(())
}

/// The value a rule gives this field: one answering a constraint it carries first, then one answering
/// its name — the first whose value fits.
pub(crate) fn named_valid(field: &FieldModel, rules: &[ValueRule]) -> Option<SampleValue> {
    let by_constraint = rules
        .iter()
        .filter(|r| field.constraints.iter().any(|c| r.constraints.iter().any(|p| matches(p, &c.name))));
    let by_name = rules.iter().filter(|r| r.fields.iter().any(|p| matches(p, &field.name)));
    by_constraint.chain(by_name).find(|r| fits(field, &r.value)).map(|r| SampleValue::Named {
        name: r.name.clone(),
        value: r.value.clone(),
        java: r.java.clone().filter(|java| !java.trim().is_empty()),
    })
}

/// The invalid value a rule gives a constraint, when one names it and has one.
pub(crate) fn named_invalid(constraint: &str, rules: &[ValueRule]) -> Option<SampleValue> {
    let rule = rules.iter().find(|r| r.invalid.is_some() && r.constraints.iter().any(|p| matches(p, constraint)))?;
    Some(SampleValue::Named {
        name: rule.name.clone(),
        value: rule.invalid.clone()?,
        java: rule.invalid_java.clone().filter(|java| !java.trim().is_empty()),
    })
}

fn normalize(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn matches(pattern: &str, name: &str) -> bool {
    let (any_prefix, rest) = match pattern.trim().strip_prefix('*') {
        Some(rest) => (true, rest),
        None => (false, pattern.trim()),
    };
    let (any_suffix, core) = match rest.strip_suffix('*') {
        Some(core) => (true, core),
        None => (false, rest),
    };
    let core = normalize(core);
    if core.is_empty() {
        return false;
    }
    let name = normalize(name);
    match (any_prefix, any_suffix) {
        (false, false) => name == core,
        (true, false) => name.ends_with(&core),
        (false, true) => name.starts_with(&core),
        (true, true) => name.contains(&core),
    }
}

/// Whether `value` can go in the field without breaking what the source says about it.
fn fits(field: &FieldModel, value: &str) -> bool {
    let has = |name: &str| field.constraints.iter().any(|c| c.name == name);
    let attr = |name: &str, key: &str| {
        field.constraints.iter().find(|c| c.name == name).and_then(|c| c.attributes.get(key)).map(String::as_str)
    };
    match shape(&field.type_name) {
        Shape::Text => {
            let length = value.chars().count() as i64;
            let min = attr("Size", "min").and_then(parse_int).unwrap_or(0);
            let max = attr("Size", "max").and_then(parse_int).unwrap_or(i64::MAX);
            let single = matches!(type_simple(&field.type_name).as_str(), "char" | "Character");
            (min..=max).contains(&length)
                && (!single || length == 1)
                && !(has("NotBlank") && value.trim().is_empty())
                && !(has("NotEmpty") && value.is_empty())
                && !(has("Email") && !value.contains('@'))
        }
        Shape::Integer => value.trim().parse::<i128>().is_ok_and(|n| within(field, n as f64)),
        Shape::Decimal => value.trim().parse::<f64>().is_ok_and(|n| n.is_finite() && within(field, n)),
        Shape::Bool => match value.trim() {
            "true" => !has("AssertFalse"),
            "false" => !has("AssertTrue"),
            _ => false,
        },
        Shape::Collection | Shape::Date | Shape::Other => false,
    }
}

fn within(field: &FieldModel, n: f64) -> bool {
    let has = |name: &str| field.constraints.iter().any(|c| c.name == name);
    let bound = |names: &[&str]| {
        field
            .constraints
            .iter()
            .filter(|c| names.contains(&c.name.as_str()))
            .find_map(|c| c.attributes.get("value"))
            .and_then(|raw| raw.trim().trim_matches('"').trim_end_matches(['L', 'l']).replace('_', "").parse::<f64>().ok())
    };
    bound(&["Min", "DecimalMin"]).map_or(true, |lower| n >= lower)
        && bound(&["Max", "DecimalMax"]).map_or(true, |upper| n <= upper)
        && !(has("Positive") && n <= 0.0)
        && !(has("PositiveOrZero") && n < 0.0)
        && !(has("Negative") && n >= 0.0)
        && !(has("NegativeOrZero") && n > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_templates::prelude::ConstraintModel;
    use serde_json::json;

    fn field(name: &str, type_name: &str, constraints: &[(&str, &[(&str, &str)])]) -> FieldModel {
        FieldModel {
            name: name.into(),
            json_name: name.into(),
            type_name: type_name.into(),
            type_simple: type_simple(type_name),
            is_final: false,
            id: false,
            annotations: Vec::new(),
            annotation_names: Vec::new(),
            ignored: false,
            setter: None,
            setter_chains: false,
            getter: None,
            wither: None,
            constraints: constraints
                .iter()
                .map(|(name, attrs)| ConstraintModel {
                    name: name.to_string(),
                    fqn: format!("com.example.{name}"),
                    attributes: attrs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
                    message: None,
                })
                .collect(),
        }
    }

    fn rule(name: &str, fields: &[&str]) -> ValueRule {
        ValueRule {
            name: name.into(),
            fields: fields.iter().map(|s| s.to_string()).collect(),
            constraints: Vec::new(),
            value: "v".into(),
            java: None,
            invalid: None,
            invalid_java: None,
        }
    }

    fn named(value: Option<SampleValue>) -> Option<String> {
        match value? {
            SampleValue::Named { name, .. } => Some(name),
            other => panic!("not a named value: {other:?}"),
        }
    }

    #[test]
    fn a_name_matches_whatever_its_case_and_separators_and_a_star_matches_the_rest() {
        assert!(matches("codiceFiscale", "CODICE_FISCALE"));
        assert!(matches("codice_fiscale", "codice-fiscale"));
        assert!(matches("*email", "billingEmail"));
        assert!(!matches("email", "billingEmail"));
        assert!(matches("indirizzo*", "indirizzoResidenza"));
        assert!(matches("*fiscal*", "codiceFiscaleAzienda"));
        assert!(!matches("*", "anything"));
    }

    /// The order of the built-ins is what makes these come out right, and it is easy to break.
    #[test]
    fn the_more_specific_built_in_answers_where_two_could() {
        let rules = builtin_rules();
        let name_of = |field_name: &str| named(named_valid(&field(field_name, "String", &[]), &rules));
        assert_eq!(name_of("emailAddress").as_deref(), Some("email"));
        assert_eq!(name_of("indirizzo_pec").as_deref(), Some("email"));
        assert_eq!(name_of("ipAddress").as_deref(), Some("ip"));
        assert_eq!(name_of("indirizzoResidenza").as_deref(), Some("address"));
        assert_eq!(name_of("zip").as_deref(), Some("postcode"));
        assert_eq!(name_of("nomeCognome").as_deref(), Some("full_name"));
        assert_eq!(name_of("codice_fiscale").as_deref(), Some("codice_fiscale"));
        assert_eq!(name_of("quantity"), None);
    }

    #[test]
    fn a_value_that_does_not_fit_the_field_is_passed_over() {
        let rules = builtin_rules();
        assert!(named_valid(&field("email", "String", &[("Size", &[("max", "5")])]), &rules).is_none());
        assert!(named_valid(&field("age", "int", &[("Min", &[("value", "40")])]), &rules).is_none());
        assert!(named_valid(&field("age", "int", &[("Min", &[("value", "18")])]), &rules).is_some());
        assert!(named_valid(&field("email", "LocalDate", &[]), &rules).is_none());
        assert!(named_valid(&field("nome", "char", &[]), &rules).is_none());
        assert!(named_valid(&field("username", "Long", &[]), &rules).is_none());
    }

    /// A custom constraint is the more reliable statement, and the one that has an invalid value to give.
    #[test]
    fn a_constraint_rule_comes_before_a_name_rule_and_gives_the_invalid_case() {
        let rules = builtin_rules();
        let f = field("email", "String", &[("CodiceFiscale", &[])]);
        assert_eq!(named(named_valid(&f, &rules)).as_deref(), Some("codice_fiscale"));
        let cases = crate::cases::invalid_cases(&f, &rules);
        assert_eq!(
            cases[0].value,
            SampleValue::Named { name: "codice_fiscale".into(), value: "RSSMRA80A01H501X".into(), java: None }
        );
        assert!(named_invalid("NotBlank", &rules).is_none());
    }

    #[test]
    fn yours_come_first_and_replace_or_switch_off_a_built_in() {
        let mut mine = rule("email", &["email"]);
        mine.value = "me@team.example".into();
        mine.java = Some("TestData.email()".into());
        let rules = effective_rules(&[mine], &["iban".to_string()]);
        assert_eq!(rules[0].value, "me@team.example");
        assert_eq!(rules.iter().filter(|r| r.name == "email").count(), 1);
        assert!(!rules.iter().any(|r| r.name == "iban"));
        let value = named_valid(&field("email", "String", &[]), &rules).unwrap();
        assert_eq!(value.to_java("String", 17), "TestData.email()");
        assert_eq!(value.to_json("String"), Some(json!("me@team.example")), "the JVM checks the literal");
    }

    #[test]
    fn every_built_in_is_well_formed_and_fits_a_plain_text_field() {
        let rules = builtin_rules();
        check_rules(&rules).unwrap();
        let text = field("anything", "String", &[]);
        for rule in &rules {
            assert!(fits(&text, &rule.value), "{}", rule.name);
        }
    }

    #[test]
    fn a_rule_that_matches_nothing_or_repeats_a_name_is_refused() {
        assert!(check_rules(&[rule("a", &["*"])]).unwrap_err().contains("matches no field"));
        assert!(check_rules(&[rule("a", &["x"]), rule("a", &["y"])]).unwrap_err().contains("Two values"));
        assert!(check_rules(&[rule("a b", &["x"])]).is_err());
    }
}
