//! What a constraint says an invalid value looks like — and, the other way round, a value that
//! satisfies every constraint on a field.
//!
//! A value is kept as **data** ([`SampleValue`]) rather than as Java text, for two consumers that
//! need different spellings of it: the JVM check wants it as JSON, and a test template wants it as
//! Java — and a template written for somebody's own helpers (`randomString(41)` instead of
//! `"x".repeat(41)`) wants the data itself, not our spelling of it. [`SampleValue::to_java`] is the
//! default spelling, offered to templates as `value_java`.
//!
//! Only the constraints of the specification are understood here. A custom one becomes a case with
//! an [`SampleValue::Unknown`] value: a template can fill it in by constraint name, and nothing is
//! invented for it.
//!
//! Both answers give way to the **value rules** ([`crate::values`]): a field called `email`, or one
//! carrying a `@CodiceFiscale`, gets the value a rule has for it — when that value fits the field.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use bennu_templates::prelude::{java_string, snake, type_simple, ConstraintModel, FieldModel};

use crate::values::{named_invalid, named_valid, ValueRule};

/// The longest string a sample is allowed to be as JSON. `@Size(max = Integer.MAX_VALUE - 1)`
/// exists, and a two-gigabyte payload is not a test.
const MAX_TEXT: u64 = 1_000_000;
const UNBOUNDED: i64 = i32::MAX as i64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SampleValue {
    Null,
    Empty,
    /// Whitespace only.
    Blank,
    StringOfLength { length: u64 },
    Integer { value: String },
    Decimal { value: String },
    Bool { value: bool },
    EmptyCollection,
    CollectionOfSize { size: u64 },
    PastDate,
    FutureDate,
    Email { valid: bool },
    Text { value: String },
    /// A value meant not to match `regexp`. Not computed — Java's regular expressions are not
    /// ours to evaluate — so the JVM check is what says whether it worked.
    NonMatching { regexp: String, value: String },
    /// A value rule's: what a field of this name, or with this constraint, is given — see
    /// [`crate::values`]. `java` is the rule's own expression for it, when it has one.
    Named { name: String, value: String, java: Option<String> },
    /// Nothing is known about the constraint; a template decides.
    Unknown,
}

/// One invalid value for one constraint on one field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Case {
    /// Unique within its field and usable in a method name: `not_blank_null`, `size_max`.
    pub name: String,
    pub constraint: String,
    pub constraint_fqn: String,
    /// The attribute the value crosses (`max`), when there is one.
    pub violated: Option<String>,
    /// The constraint's attributes, without `message`, `groups` and `payload`.
    pub attributes: BTreeMap<String, String>,
    pub value: SampleValue,
}

/// The shape of a Java type, as far as choosing a value for it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Text,
    Integer,
    Decimal,
    Bool,
    Collection,
    Date,
    Other,
}

pub fn shape(type_name: &str) -> Shape {
    let simple = type_simple(type_name);
    if simple.ends_with("[]") {
        return Shape::Collection;
    }
    match simple.as_str() {
        "String" | "CharSequence" | "char" | "Character" => Shape::Text,
        "int" | "Integer" | "long" | "Long" | "short" | "Short" | "byte" | "Byte" | "BigInteger" => {
            Shape::Integer
        }
        "double" | "Double" | "float" | "Float" | "BigDecimal" => Shape::Decimal,
        "boolean" | "Boolean" => Shape::Bool,
        "List" | "ArrayList" | "LinkedList" | "Collection" | "Iterable" | "Set" | "HashSet"
        | "LinkedHashSet" | "TreeSet" | "SortedSet" | "Map" | "HashMap" | "LinkedHashMap"
        | "TreeMap" | "SortedMap" => Shape::Collection,
        "LocalDate" | "LocalDateTime" | "LocalTime" | "Instant" | "ZonedDateTime"
        | "OffsetDateTime" | "OffsetTime" | "Date" | "Calendar" | "Year" | "YearMonth" => Shape::Date,
        _ => Shape::Other,
    }
}

fn is_primitive(simple: &str) -> bool {
    matches!(simple, "int" | "long" | "short" | "byte" | "double" | "float" | "boolean" | "char")
}

fn is_map(simple: &str) -> bool {
    simple.ends_with("Map")
}

fn is_set(simple: &str) -> bool {
    simple.ends_with("Set")
}

/// Every invalid value the field's constraints imply, in constraint order. A constraint this does not
/// know takes the invalid value of a rule naming it.
pub fn invalid_cases(field: &FieldModel, rules: &[ValueRule]) -> Vec<Case> {
    let shape = shape(&field.type_name);
    let simple = type_simple(&field.type_name);
    let mut out: Vec<Case> = Vec::new();
    for c in &field.constraints {
        for (suffix, violated, value) in violating(c, shape, &simple) {
            let base = snake(&c.name);
            let name = match suffix {
                Some(suffix) => format!("{base}_{suffix}"),
                None => base,
            };
            out.push(Case {
                name: unique(&out, name),
                constraint: c.name.clone(),
                constraint_fqn: c.fqn.clone(),
                violated: violated.map(str::to_string),
                attributes: public_attributes(&c.attributes),
                value: match value {
                    SampleValue::Unknown => named_invalid(&c.name, rules).unwrap_or(SampleValue::Unknown),
                    known => known,
                },
            });
        }
    }
    out
}

fn unique(existing: &[Case], name: String) -> String {
    if !existing.iter().any(|c| c.name == name) {
        return name;
    }
    (2..)
        .map(|i| format!("{name}_{i}"))
        .find(|candidate| !existing.iter().any(|c| &c.name == candidate))
        .unwrap_or(name)
}

/// The constraint's attributes that say something about the value.
pub(crate) fn public_attributes(all: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    all.iter()
        .filter(|(k, _)| !matches!(k.as_str(), "message" | "groups" | "payload"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

type Violating = (Option<&'static str>, Option<&'static str>, SampleValue);

fn violating(c: &ConstraintModel, shape: Shape, simple: &str) -> Vec<Violating> {
    use SampleValue::*;
    let attr = |key: &str| c.attributes.get(key).map(String::as_str);
    let int = |key: &str| attr(key).and_then(parse_int);
    match c.name.as_str() {
        "NotNull" if is_primitive(simple) => Vec::new(),
        "NotNull" => vec![(None, None, Null)],
        "NotBlank" => vec![(Some("null"), None, Null), (Some("blank"), None, Blank)],
        "NotEmpty" => match shape {
            Shape::Collection => vec![(Some("null"), None, Null), (Some("empty"), None, EmptyCollection)],
            _ => vec![(Some("null"), None, Null), (Some("empty"), None, Empty)],
        },
        "Size" => {
            let min = int("min").unwrap_or(0);
            let max = int("max").unwrap_or(UNBOUNDED);
            let mut out = Vec::new();
            if min > 0 {
                out.push((Some("min"), Some("min"), sized(shape, (min - 1) as u64)));
            }
            if max < UNBOUNDED && max >= 0 {
                out.push((Some("max"), Some("max"), sized(shape, max as u64 + 1)));
            }
            out
        }
        "Min" | "DecimalMin" => {
            beyond(attr("value"), -1, shape).map(|v| vec![(None, Some("value"), v)]).unwrap_or_default()
        }
        "Max" | "DecimalMax" => {
            beyond(attr("value"), 1, shape).map(|v| vec![(None, Some("value"), v)]).unwrap_or_default()
        }
        "Positive" | "Negative" => vec![(None, None, number(shape, "0"))],
        "PositiveOrZero" => vec![(None, None, number(shape, "-1"))],
        "NegativeOrZero" => vec![(None, None, number(shape, "1"))],
        "Past" | "PastOrPresent" => vec![(None, None, FutureDate)],
        "Future" | "FutureOrPresent" => vec![(None, None, PastDate)],
        "Email" => vec![(None, None, Email { valid: false })],
        "Pattern" => vec![(
            None,
            Some("regexp"),
            NonMatching { regexp: attr("regexp").unwrap_or_default().to_string(), value: "!".into() },
        )],
        "AssertTrue" => vec![(None, None, Bool { value: false })],
        "AssertFalse" => vec![(None, None, Bool { value: true })],
        "Digits" => int("integer")
            .map(|n| {
                let digits = "9".repeat(n.clamp(0, 30) as usize + 1);
                vec![(Some("integer"), Some("integer"), number(shape, &digits))]
            })
            .unwrap_or_default(),
        "Null" => Vec::new(),
        _ => vec![(None, None, Unknown)],
    }
}

fn sized(shape: Shape, n: u64) -> SampleValue {
    match shape {
        Shape::Collection => SampleValue::CollectionOfSize { size: n },
        _ => SampleValue::StringOfLength { length: n },
    }
}

fn number(shape: Shape, value: &str) -> SampleValue {
    match shape {
        Shape::Decimal => SampleValue::Decimal { value: value.to_string() },
        _ => SampleValue::Integer { value: value.to_string() },
    }
}

/// One step past a bound written as `1`, `10L` or `"0.5"`. `None` for a bound this cannot read as a
/// number — a constant, say — which is a case not worth guessing at.
fn beyond(raw: Option<&str>, step: i64, shape: Shape) -> Option<SampleValue> {
    let raw = raw?;
    if let Some(n) = parse_int(raw) {
        return Some(number(shape, &(i128::from(n) + i128::from(step)).to_string()));
    }
    let f: f64 = raw.trim().trim_matches('"').parse().ok()?;
    Some(SampleValue::Decimal { value: format!("{}", f + step as f64) })
}

pub(crate) fn parse_int(raw: &str) -> Option<i64> {
    raw.trim().trim_matches('"').trim_end_matches(['L', 'l']).replace('_', "").parse().ok()
}

/// A bound as a whole number, rounded toward the inside of the range it bounds.
fn parse_bound(raw: &str, lower: bool) -> Option<i128> {
    if let Some(n) = parse_int(raw) {
        return Some(i128::from(n));
    }
    let f: f64 = raw.trim().trim_matches('"').parse().ok()?;
    Some(if lower { f.ceil() } else { f.floor() } as i128)
}

/// A value every constraint on the field accepts — what a valid instance is built from.
///
/// [`SampleValue::Unknown`] where none can be chosen without guessing: a `@Pattern`, a custom
/// constraint on a type this does not know. The JVM check of the valid instance then says which.
///
/// A value rule answering the field comes before all of it.
pub fn valid_value(field: &FieldModel, rules: &[ValueRule]) -> SampleValue {
    use SampleValue::*;
    let find = |name: &str| field.constraints.iter().find(|c| c.name == name);
    let has = |name: &str| find(name).is_some();
    let attr = |name: &str, key: &str| find(name).and_then(|c| c.attributes.get(key)).map(String::as_str);
    if has("Null") {
        return Null;
    }
    if let Some(named) = named_valid(field, rules) {
        return named;
    }
    let shape = shape(&field.type_name);
    match shape {
        Shape::Text => {
            if has("Email") {
                return Email { valid: true };
            }
            if has("Pattern") {
                return Unknown;
            }
            let min = attr("Size", "min").and_then(parse_int).unwrap_or(0);
            let max = attr("Size", "max").and_then(parse_int).unwrap_or(UNBOUNDED);
            let length = min.max(1).min(max.max(0));
            match length {
                1 => Text { value: "x".into() },
                n => StringOfLength { length: n.max(0) as u64 },
            }
        }
        Shape::Integer | Shape::Decimal => {
            let mut candidate: i128 = if has("Negative") {
                -1
            } else if has("NegativeOrZero") {
                0
            } else {
                1
            };
            let lower = attr("Min", "value").or(attr("DecimalMin", "value")).and_then(|v| parse_bound(v, true));
            let upper = attr("Max", "value").or(attr("DecimalMax", "value")).and_then(|v| parse_bound(v, false));
            if let Some(lower) = lower {
                candidate = candidate.max(lower);
            }
            if let Some(upper) = upper {
                candidate = candidate.min(upper);
            }
            number(shape, &candidate.to_string())
        }
        Shape::Bool => Bool { value: !has("AssertFalse") },
        Shape::Collection => {
            let min = attr("Size", "min").and_then(parse_int).unwrap_or(0).max(i64::from(has("NotEmpty")));
            match min {
                0 => EmptyCollection,
                n => CollectionOfSize { size: n as u64 },
            }
        }
        Shape::Date => match has("Future") || has("FutureOrPresent") {
            true => FutureDate,
            false => PastDate,
        },
        Shape::Other => match has("NotNull") {
            true => Unknown,
            false => Null,
        },
    }
}

impl SampleValue {
    /// The value as JSON, for binding on the JVM. `None` for [`SampleValue::Unknown`].
    pub fn to_json(&self, type_simple: &str) -> Option<Value> {
        use SampleValue::*;
        Some(match self {
            Null => Value::Null,
            Empty => json!(""),
            Blank => json!("   "),
            StringOfLength { length } => Value::String("x".repeat((*length).min(MAX_TEXT) as usize)),
            Integer { value } => {
                value.parse::<i64>().map(Value::from).unwrap_or_else(|_| Value::String(value.clone()))
            }
            Decimal { value } => value
                .parse::<f64>()
                .ok()
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number)
                .unwrap_or_else(|| Value::String(value.clone())),
            Bool { value } => Value::Bool(*value),
            EmptyCollection => match is_map(type_simple) {
                true => json!({}),
                false => json!([]),
            },
            // Distinct elements, or a set collapses them and the size is wrong.
            CollectionOfSize { size } => {
                let n = (*size).min(MAX_TEXT);
                match is_map(type_simple) {
                    true => Value::Object((0..n).map(|i| (i.to_string(), json!(i))).collect()),
                    false => Value::Array((0..n).map(|i| json!(i)).collect()),
                }
            }
            PastDate => date_json(type_simple, true),
            FutureDate => date_json(type_simple, false),
            Email { valid } => json!(if *valid { "user@example.com" } else { "not-an-email" }),
            Text { value } | NonMatching { value, .. } => Value::String(value.clone()),
            Named { value, .. } => typed_json(value, type_simple),
            Unknown => return None,
        })
    }

    /// The value spelled in Java, for a field of this type and a project at this language level.
    pub fn to_java(&self, type_simple: &str, java: u32) -> String {
        use SampleValue::*;
        match self {
            Null => "null".into(),
            Empty => "\"\"".into(),
            Blank => "\"   \"".into(),
            StringOfLength { length } => match java >= 11 {
                true => format!("\"x\".repeat({length})"),
                false => format!("new String(new char[{length}]).replace('\\0', 'x')"),
            },
            Integer { value } | Decimal { value } => number_java(value, type_simple),
            Bool { value } => value.to_string(),
            EmptyCollection => empty_collection_java(type_simple),
            CollectionOfSize { size } => sized_collection_java(type_simple, *size),
            PastDate => date_java(type_simple, true),
            FutureDate => date_java(type_simple, false),
            Email { valid } => java_string(if *valid { "user@example.com" } else { "not-an-email" }),
            Text { value } | NonMatching { value, .. } => java_string(value),
            Named { java: Some(expression), .. } if !expression.trim().is_empty() => expression.clone(),
            Named { value, .. } => typed_java(value, type_simple),
            Unknown => "null /* TODO: a value for this constraint */".into(),
        }
    }
}

fn number_java(value: &str, type_simple: &str) -> String {
    match type_simple {
        "long" | "Long" => format!("{value}L"),
        "double" | "Double" => format!("{value}d"),
        "float" | "Float" => format!("{value}f"),
        "short" | "Short" => format!("(short) {value}"),
        "byte" | "Byte" => format!("(byte) {value}"),
        "BigDecimal" => format!("new java.math.BigDecimal(\"{value}\")"),
        "BigInteger" => format!("new java.math.BigInteger(\"{value}\")"),
        _ => value.to_string(),
    }
}

/// A rule's text as JSON of the field's type — `"30"` is a number to an `int`.
fn typed_json(text: &str, type_simple: &str) -> Value {
    let raw = || Value::String(text.to_string());
    match shape(type_simple) {
        Shape::Integer => text.trim().parse::<i64>().map(Value::from).unwrap_or_else(|_| raw()),
        Shape::Decimal => text.trim().parse::<f64>().ok().and_then(serde_json::Number::from_f64).map(Value::Number).unwrap_or_else(raw),
        Shape::Bool => text.trim().parse::<bool>().map(Value::Bool).unwrap_or_else(|_| raw()),
        _ => raw(),
    }
}

/// A rule's text spelled in Java for the field's type. A whole number is re-spelled from its value, so a
/// postcode written `00184` does not become an octal literal that does not compile.
fn typed_java(text: &str, type_simple: &str) -> String {
    match shape(type_simple) {
        Shape::Integer => match text.trim().parse::<i128>() {
            Ok(n) => number_java(&n.to_string(), type_simple),
            Err(_) => java_string(text),
        },
        Shape::Decimal => number_java(text.trim(), type_simple),
        Shape::Bool => text.trim().to_string(),
        _ if matches!(type_simple, "char" | "Character") && text.chars().count() == 1 => match text {
            "'" => "'\\''".to_string(),
            "\\" => "'\\\\'".to_string(),
            c => format!("'{c}'"),
        },
        _ => java_string(text),
    }
}

fn empty_collection_java(type_simple: &str) -> String {
    if let Some(element) = type_simple.strip_suffix("[]") {
        return format!("new {element}[0]");
    }
    if is_map(type_simple) {
        "new java.util.HashMap<>()".into()
    } else if is_set(type_simple) {
        "new java.util.HashSet<>()".into()
    } else {
        "new java.util.ArrayList<>()".into()
    }
}

fn sized_collection_java(type_simple: &str, size: u64) -> String {
    if let Some(element) = type_simple.strip_suffix("[]") {
        return format!("new {element}[{size}]");
    }
    let range = format!("java.util.stream.IntStream.range(0, {size}).boxed()");
    if is_map(type_simple) {
        format!("(java.util.Map) {range}.collect(java.util.stream.Collectors.toMap(i -> i, i -> i))")
    } else if is_set(type_simple) {
        format!("(java.util.Set) {range}.collect(java.util.stream.Collectors.toSet())")
    } else {
        format!("new java.util.ArrayList<>(java.util.Collections.nCopies({size}, null))")
    }
}

fn date_json(type_simple: &str, past: bool) -> Value {
    let (date, millis) = match past {
        true => ("2000-01-01", 946_684_800_000_i64),
        false => ("2999-01-01", 32_503_680_000_000_i64),
    };
    match type_simple {
        "LocalDateTime" => json!(format!("{date}T00:00:00")),
        "Instant" | "ZonedDateTime" | "OffsetDateTime" => json!(format!("{date}T00:00:00Z")),
        "Date" | "Calendar" => json!(millis),
        "Year" => json!(&date[..4]),
        "YearMonth" => json!(&date[..7]),
        _ => json!(date),
    }
}

fn date_java(type_simple: &str, past: bool) -> String {
    let way = if past { "minus" } else { "plus" };
    match type_simple {
        "LocalDate" | "LocalDateTime" | "ZonedDateTime" | "OffsetDateTime" => {
            format!("java.time.{type_simple}.now().{way}Days(1)")
        }
        "Instant" => format!("java.time.Instant.now().{way}Seconds(86400)"),
        "Year" => format!("java.time.Year.now().{way}Years(1)"),
        "YearMonth" => format!("java.time.YearMonth.now().{way}Months(1)"),
        "Date" if past => "new java.util.Date(0)".into(),
        "Date" => "new java.util.Date(System.currentTimeMillis() + 86_400_000L)".into(),
        _ => format!("null /* TODO: a {} date */", if past { "past" } else { "future" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The cases here are about the constraints alone; the rules have their own tests in `values`.
    fn valid_value(f: &FieldModel) -> SampleValue {
        super::valid_value(f, &[])
    }
    fn invalid_cases(f: &FieldModel) -> Vec<Case> {
        super::invalid_cases(f, &[])
    }

    fn field(type_name: &str, constraints: &[(&str, &[(&str, &str)])]) -> FieldModel {
        FieldModel {
            name: "value".into(),
            json_name: "value".into(),
            type_name: type_name.into(),
            type_simple: type_simple(type_name),
            is_final: false,
            id: false,
            annotations: Vec::new(),
            annotation_names: Vec::new(),
            ignored: false,
            setter: Some("setValue".into()),
            setter_chains: false,
            getter: None,
            wither: None,
            constraints: constraints
                .iter()
                .map(|(name, attrs)| ConstraintModel {
                    name: name.to_string(),
                    fqn: format!("jakarta.validation.constraints.{name}"),
                    attributes: attrs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
                    message: None,
                })
                .collect(),
        }
    }

    #[test]
    fn a_size_is_crossed_on_both_sides_that_exist() {
        let f = field("String", &[("Size", &[("min", "2"), ("max", "40")])]);
        let cases = invalid_cases(&f);
        let found: Vec<(&str, &SampleValue)> = cases.iter().map(|c| (c.name.as_str(), &c.value)).collect();
        assert_eq!(
            found,
            [
                ("size_min", &SampleValue::StringOfLength { length: 1 }),
                ("size_max", &SampleValue::StringOfLength { length: 41 }),
            ]
        );
        assert_eq!(cases[1].violated.as_deref(), Some("max"));
    }

    #[test]
    fn an_unbounded_size_has_no_upper_case_and_a_zero_minimum_no_lower_one() {
        assert!(invalid_cases(&field("String", &[("Size", &[])])).is_empty());
    }

    #[test]
    fn not_null_on_a_primitive_cannot_be_violated_and_is_not_pretended_to() {
        assert!(invalid_cases(&field("int", &[("NotNull", &[])])).is_empty());
        assert_eq!(invalid_cases(&field("Integer", &[("NotNull", &[])]))[0].value, SampleValue::Null);
    }

    #[test]
    fn a_numeric_bound_is_stepped_past_whatever_way_it_is_written() {
        let min = invalid_cases(&field("long", &[("Min", &[("value", "10L")])]));
        assert_eq!(min[0].value, SampleValue::Integer { value: "9".into() });
        let max = invalid_cases(&field("BigDecimal", &[("DecimalMax", &[("value", "0.5")])]));
        assert_eq!(max[0].value, SampleValue::Decimal { value: "1.5".into() });
    }

    /// A custom constraint is not guessed at — the case exists, and its value is the template's call.
    #[test]
    fn a_constraint_this_does_not_know_is_a_case_with_an_unknown_value() {
        let cases = invalid_cases(&field("String", &[("TaxCode", &[])]));
        assert_eq!(cases[0].name, "tax_code");
        assert_eq!(cases[0].value, SampleValue::Unknown);
        assert_eq!(cases[0].value.to_json("String"), None);
    }

    #[test]
    fn two_cases_from_the_same_constraint_get_different_names() {
        let f = field("String", &[("Pattern", &[("regexp", "a")]), ("Pattern", &[("regexp", "b")])]);
        let names: Vec<String> = invalid_cases(&f).into_iter().map(|c| c.name).collect();
        assert_eq!(names, ["pattern", "pattern_2"]);
    }

    #[test]
    fn a_valid_value_sits_inside_every_bound() {
        assert_eq!(
            valid_value(&field("String", &[("Size", &[("min", "3"), ("max", "10")])])),
            SampleValue::StringOfLength { length: 3 }
        );
        assert_eq!(
            valid_value(&field("int", &[("Min", &[("value", "18")]), ("Max", &[("value", "99")])])),
            SampleValue::Integer { value: "18".into() }
        );
        assert_eq!(
            valid_value(&field("List<String>", &[("NotEmpty", &[])])),
            SampleValue::CollectionOfSize { size: 1 }
        );
        assert_eq!(valid_value(&field("String", &[("Email", &[])])), SampleValue::Email { valid: true });
    }

    #[test]
    fn a_value_is_spelled_for_its_type_and_the_projects_java() {
        let long_text = SampleValue::StringOfLength { length: 41 };
        assert_eq!(long_text.to_java("String", 17), "\"x\".repeat(41)");
        assert_eq!(long_text.to_java("String", 8), "new String(new char[41]).replace('\\0', 'x')");
        assert_eq!(SampleValue::Integer { value: "9".into() }.to_java("Long", 17), "9L");
        assert_eq!(
            SampleValue::Decimal { value: "1.5".into() }.to_java("BigDecimal", 17),
            "new java.math.BigDecimal(\"1.5\")"
        );
        assert_eq!(SampleValue::PastDate.to_java("LocalDate", 17), "java.time.LocalDate.now().minusDays(1)");
    }

    #[test]
    fn a_set_of_n_elements_is_n_distinct_elements_in_json() {
        let json = SampleValue::CollectionOfSize { size: 3 }.to_json("Set").unwrap();
        assert_eq!(json, json!([0, 1, 2]));
    }

    #[test]
    fn a_named_value_is_spelled_for_its_type_or_by_its_own_expression() {
        let named = |value: &str, java: Option<&str>| SampleValue::Named {
            name: "n".into(),
            value: value.into(),
            java: java.map(str::to_string),
        };
        assert_eq!(named("00184", None).to_java("int", 17), "184");
        assert_eq!(named("00184", None).to_json("Integer"), Some(json!(184)));
        assert_eq!(named("RSSMRA80A01H501U", None).to_java("String", 17), "\"RSSMRA80A01H501U\"");
        assert_eq!(named("M", None).to_java("char", 17), "'M'");
        assert_eq!(named("x", Some("TestData.cf()")).to_java("String", 17), "TestData.cf()");
    }

    #[test]
    fn a_type_is_reduced_to_the_simple_name_a_choice_is_made_on() {
        assert_eq!(type_simple("java.util.List<java.lang.String>"), "List");
        assert_eq!(type_simple("String[]"), "String[]");
        assert_eq!(shape("java.time.LocalDate"), Shape::Date);
    }
}
