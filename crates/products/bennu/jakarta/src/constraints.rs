//! The constraints themselves — what each one is called, what attributes it carries, and what it
//! can be put on.
//!
//! ## Why a table and not a lookup through the index
//!
//! Because these are the constraints that come **in the jar**, and the jar is where their
//! definitions are: `@NotNull`'s attributes are not in any source this project owns. The index
//! resolves what a project declares; this is the vocabulary it declares against, and it is a
//! closed, versioned, small set that has barely changed since 2009.
//!
//! ## The attribute list is load-bearing, not documentation
//!
//! Bean Validation interpolates a message by resolving `{name}` **against the constraint's own
//! attributes first**, and only then against a bundle. So `@Size(message = "between {min} and
//! {max}")` names no bundle keys at all — and a check that read every `{…}` as a key would report
//! two missing keys on every custom `@Size` message in the project, which is precisely the kind of
//! confidently-wrong output that makes people turn a check off. The table is what tells the two
//! apart.
//!
//! ## What "cannot be applied" means here
//!
//! Hibernate Validator throws `UnexpectedTypeException` **at startup** for a constraint on a type
//! it has no validator for — `@NotBlank` on an `int`, `@Size` on a `long`. That is worth saying in
//! the editor, but only where it is certain, so [`verdict`] answers [`Verdict::Unknown`] for every
//! type it does not recognise: a project type, a generic parameter, a JDK class not in the short
//! list below. Under-report rather than risk a false positive (docs §7) — a check that flags a
//! valid custom `ConstraintValidator` is a check nobody keeps.

/// What a constraint accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accepts {
    /// Anything at all — `@Null`, and the composed ones.
    Anything,
    /// A `CharSequence`. `@NotBlank`, `@Email`, `@Pattern`.
    Text,
    /// Something with a size: `CharSequence`, `Collection`, `Map`, array. `@Size`, `@NotEmpty`.
    Sized,
    /// `boolean` / `Boolean`. `@AssertTrue`, `@AssertFalse`.
    Truth,
    /// A number. `@Min`, `@Max`, `@Positive`, `@Digits`.
    Number,
    /// A number **or** a numeric `CharSequence` — the `@DecimalMin` / `@DecimalMax` family, which
    /// is the reason this is not the same as [`Accepts::Number`].
    NumberOrText,
    /// A date or a time. `@Past`, `@Future`.
    Moment,
    /// Anything that can BE null — which is everything except a primitive.
    Nullable,
}

/// One constraint annotation.
#[derive(Debug, Clone, Copy)]
pub struct Constraint {
    /// Simple name, as written (`"NotNull"`).
    pub name: &'static str,
    /// The package it is declared in, in its Jakarta spelling. A `javax` project writes the same
    /// simple name, and [`default_message_keys`] answers for both.
    pub package: &'static str,
    /// The attributes it declares **beyond** `message`, `groups` and `payload`, which every
    /// constraint has. These are the names a message may interpolate without naming a bundle key.
    pub attributes: &'static [&'static str],
    pub accepts: Accepts,
    /// One line, for the hover.
    pub doc: &'static str,
}

const JV: &str = "jakarta.validation.constraints";
const HV: &str = "org.hibernate.validator.constraints";

/// The Bean Validation 3.0 built-ins, plus the Hibernate Validator constraints that turn up in
/// practically every project that uses the engine at all.
///
/// `javax.validation` (Bean Validation 2.0 and earlier) declares the same simple names in the same
/// shapes; the two spellings differ only in the package, which is why nothing here is duplicated
/// for them.
pub const CONSTRAINTS: &[Constraint] = &[
    Constraint { name: "AssertFalse", package: JV, attributes: &[], accepts: Accepts::Truth,
        doc: "The value must be false." },
    Constraint { name: "AssertTrue", package: JV, attributes: &[], accepts: Accepts::Truth,
        doc: "The value must be true." },
    Constraint { name: "DecimalMax", package: JV, attributes: &["value", "inclusive"], accepts: Accepts::NumberOrText,
        doc: "The value must be no greater than the given decimal." },
    Constraint { name: "DecimalMin", package: JV, attributes: &["value", "inclusive"], accepts: Accepts::NumberOrText,
        doc: "The value must be no less than the given decimal." },
    Constraint { name: "Digits", package: JV, attributes: &["integer", "fraction"], accepts: Accepts::NumberOrText,
        doc: "The value must be a number within the given integer and fraction digit counts." },
    Constraint { name: "Email", package: JV, attributes: &["regexp", "flags"], accepts: Accepts::Text,
        doc: "The value must be a well-formed email address." },
    Constraint { name: "Future", package: JV, attributes: &[], accepts: Accepts::Moment,
        doc: "The value must be an instant in the future." },
    Constraint { name: "FutureOrPresent", package: JV, attributes: &[], accepts: Accepts::Moment,
        doc: "The value must be an instant in the future or the present." },
    Constraint { name: "Max", package: JV, attributes: &["value"], accepts: Accepts::Number,
        doc: "The value must be no greater than the given maximum." },
    Constraint { name: "Min", package: JV, attributes: &["value"], accepts: Accepts::Number,
        doc: "The value must be no less than the given minimum." },
    Constraint { name: "Negative", package: JV, attributes: &[], accepts: Accepts::Number,
        doc: "The value must be strictly negative." },
    Constraint { name: "NegativeOrZero", package: JV, attributes: &[], accepts: Accepts::Number,
        doc: "The value must be negative or zero." },
    Constraint { name: "NotBlank", package: JV, attributes: &[], accepts: Accepts::Text,
        doc: "The text must not be null and must contain at least one non-whitespace character." },
    Constraint { name: "NotEmpty", package: JV, attributes: &[], accepts: Accepts::Sized,
        doc: "The value must not be null and must not be empty." },
    Constraint { name: "NotNull", package: JV, attributes: &[], accepts: Accepts::Nullable,
        doc: "The value must not be null." },
    Constraint { name: "Null", package: JV, attributes: &[], accepts: Accepts::Anything,
        doc: "The value must be null." },
    Constraint { name: "Past", package: JV, attributes: &[], accepts: Accepts::Moment,
        doc: "The value must be an instant in the past." },
    Constraint { name: "PastOrPresent", package: JV, attributes: &[], accepts: Accepts::Moment,
        doc: "The value must be an instant in the past or the present." },
    Constraint { name: "Pattern", package: JV, attributes: &["regexp", "flags"], accepts: Accepts::Text,
        doc: "The text must match the given regular expression." },
    Constraint { name: "Positive", package: JV, attributes: &[], accepts: Accepts::Number,
        doc: "The value must be strictly positive." },
    Constraint { name: "PositiveOrZero", package: JV, attributes: &[], accepts: Accepts::Number,
        doc: "The value must be positive or zero." },
    Constraint { name: "Size", package: JV, attributes: &["min", "max"], accepts: Accepts::Sized,
        doc: "The value's size must be between the given bounds." },
    // ── Hibernate Validator ──────────────────────────────────────────────────
    Constraint { name: "CreditCardNumber", package: HV, attributes: &["ignoreNonDigitCharacters"], accepts: Accepts::Text,
        doc: "The text must pass the Luhn check for a credit card number." },
    Constraint { name: "EAN", package: HV, attributes: &["type"], accepts: Accepts::Text,
        doc: "The text must be a valid EAN barcode." },
    Constraint { name: "ISBN", package: HV, attributes: &["type"], accepts: Accepts::Text,
        doc: "The text must be a valid ISBN." },
    Constraint { name: "Length", package: HV, attributes: &["min", "max"], accepts: Accepts::Text,
        doc: "The text's length must be between the given bounds." },
    Constraint { name: "Range", package: HV, attributes: &["min", "max"], accepts: Accepts::NumberOrText,
        doc: "The value must be between the given bounds, inclusive." },
    Constraint { name: "URL", package: HV, attributes: &["protocol", "host", "port", "regexp", "flags"], accepts: Accepts::Text,
        doc: "The text must be a well-formed URL." },
    Constraint { name: "UniqueElements", package: HV, attributes: &[], accepts: Accepts::Sized,
        doc: "The collection's elements must all be different." },
];

/// The constraint of that simple name, if it is one.
pub fn constraint(name: &str) -> Option<&'static Constraint> {
    CONSTRAINTS.iter().find(|c| c.name == name)
}

/// Whether `name` is a constraint annotation this crate knows.
pub fn is_constraint(name: &str) -> bool {
    constraint(name).is_some()
}

impl Constraint {
    /// The names a message may interpolate that are **not** bundle keys: this constraint's own
    /// attributes, plus the three every constraint carries.
    pub fn interpolates(&self, name: &str) -> bool {
        self.attributes.contains(&name) || matches!(name, "message" | "groups" | "payload")
    }

    /// The default message key, in both spellings a project can be written in.
    ///
    /// Both, because the key lives in the **provider's** bundle inside the jar, and a project that
    /// migrated from `javax` to `jakarta` regularly still has messages written against the old
    /// one — which resolve, because the old jar is still on the classpath.
    pub fn default_message_keys(&self) -> [String; 2] {
        [
            format!("{}.{}.message", self.package, self.name),
            format!("{}.{}.message", self.package.replace("jakarta.", "javax."), self.name),
        ]
    }
}

/// Every key the **provider's own** bundles declare — the ones a message may reference without the
/// project declaring anything.
pub fn provider_keys() -> Vec<String> {
    CONSTRAINTS.iter().flat_map(|c| c.default_message_keys()).collect()
}

/// Whether a key is one the provider answers. A message referencing one of these is correct and
/// must never be reported as missing, however empty the project's own bundles are.
pub fn is_provider_key(key: &str) -> bool {
    let Some(rest) = key.strip_suffix(".message") else { return false };
    let Some((package, name)) = rest.rsplit_once('.') else { return false };
    let Some(c) = constraint(name) else { return false };
    package == c.package || package == c.package.replace("jakarta.", "javax.")
}

// ── applicability ────────────────────────────────────────────────────────────

/// What can be said about putting a constraint on a member of a given written type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The engine has a validator for this pairing.
    Fits,
    /// It does not, and Hibernate Validator will refuse at startup. The string says why.
    Cannot(&'static str),
    /// It resolves, but it can never fail — `@NotNull` on a primitive.
    Pointless(&'static str),
    /// Nothing certain can be said: a project type, a type variable, a JDK class not in the short
    /// list. Silence.
    Unknown,
}

/// The shape of a written type, for the only types this dares to have an opinion about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Primitive(Kind),
    Boxed(Kind),
    Text,
    Collection,
    Array,
    Moment,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Truth,
    Number,
    Char,
}

/// Classify a written type. Deliberately narrow: everything it is not certain about is
/// [`Shape::Unknown`], which produces no diagnostic anywhere.
fn shape_of(written: &str) -> Shape {
    let t = written.trim();
    if t.ends_with(']') && t.contains('[') {
        return Shape::Array;
    }
    // A generic type is classified by its raw name — `List<Order>` is a collection whatever it
    // holds, and `Optional<String>` is not something this has an opinion about.
    let raw = t.split_once('<').map(|(head, _)| head).unwrap_or(t).trim();
    match raw {
        "boolean" => Shape::Primitive(Kind::Truth),
        "char" => Shape::Primitive(Kind::Char),
        "byte" | "short" | "int" | "long" | "float" | "double" => Shape::Primitive(Kind::Number),
        "Boolean" => Shape::Boxed(Kind::Truth),
        "Character" => Shape::Boxed(Kind::Char),
        "Byte" | "Short" | "Integer" | "Long" | "Float" | "Double" | "BigDecimal" | "BigInteger" => {
            Shape::Boxed(Kind::Number)
        }
        "String" | "CharSequence" | "StringBuilder" => Shape::Text,
        "Collection" | "List" | "ArrayList" | "Set" | "HashSet" | "LinkedHashSet" | "SortedSet"
        | "TreeSet" | "Map" | "HashMap" | "LinkedHashMap" | "SortedMap" | "TreeMap" => {
            Shape::Collection
        }
        "Date" | "Calendar" | "Instant" | "LocalDate" | "LocalDateTime" | "LocalTime"
        | "OffsetDateTime" | "OffsetTime" | "ZonedDateTime" | "Year" | "YearMonth"
        | "MonthDay" | "HijrahDate" | "JapaneseDate" | "MinguoDate" | "ThaiBuddhistDate" => {
            Shape::Moment
        }
        _ => Shape::Unknown,
    }
}

/// What can be said about `constraint` on a member written as `written_type`.
pub fn verdict(constraint: &Constraint, written_type: &str) -> Verdict {
    let shape = shape_of(written_type);
    // A primitive is never null, so the two null constraints resolve and decide nothing. Not a
    // startup failure — a line that reads as a rule and is not one.
    if let Shape::Primitive(_) = shape {
        match constraint.name {
            "NotNull" => return Verdict::Pointless("a primitive is never null"),
            "Null" => return Verdict::Pointless("a primitive is never null, so this always fails"),
            _ => {}
        }
    }
    // The two that accept anything are the two where an unrecognised type is still an answer: a
    // name this does not know is a *reference* type, and every reference can be null. Checked
    // before the `Unknown` gate below for exactly that reason.
    if matches!(constraint.accepts, Accepts::Anything | Accepts::Nullable) {
        return Verdict::Fits;
    }
    if shape == Shape::Unknown {
        return Verdict::Unknown;
    }
    let fits = match constraint.accepts {
        Accepts::Anything | Accepts::Nullable => true,
        Accepts::Text => shape == Shape::Text,
        Accepts::Sized => matches!(shape, Shape::Text | Shape::Collection | Shape::Array),
        Accepts::Truth => matches!(shape, Shape::Primitive(Kind::Truth) | Shape::Boxed(Kind::Truth)),
        Accepts::Number => {
            matches!(shape, Shape::Primitive(Kind::Number) | Shape::Boxed(Kind::Number))
        }
        Accepts::NumberOrText => matches!(
            shape,
            Shape::Primitive(Kind::Number) | Shape::Boxed(Kind::Number) | Shape::Text
        ),
        Accepts::Moment => shape == Shape::Moment,
    };
    if fits {
        return Verdict::Fits;
    }
    Verdict::Cannot(match constraint.accepts {
        Accepts::Text => "it applies to text",
        Accepts::Sized => "it applies to text, a collection, a map or an array",
        Accepts::Truth => "it applies to a boolean",
        Accepts::Number => "it applies to a number",
        Accepts::NumberOrText => "it applies to a number or to text holding one",
        Accepts::Moment => "it applies to a date or a time",
        Accepts::Anything | Accepts::Nullable => "",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_attributes_are_what_keeps_a_message_placeholder_from_looking_like_a_key() {
        let size = constraint("Size").unwrap();
        assert!(size.interpolates("min"));
        assert!(size.interpolates("max"));
        // Every constraint carries these three, and a message may name them.
        assert!(size.interpolates("groups"));
        // This one is a bundle key, and the whole check depends on the difference.
        assert!(!size.interpolates("order.name.tooLong"));
    }

    #[test]
    fn a_providers_own_key_is_recognised_in_both_spellings() {
        assert!(is_provider_key("jakarta.validation.constraints.NotNull.message"));
        assert!(is_provider_key("javax.validation.constraints.NotNull.message"));
        assert!(is_provider_key("org.hibernate.validator.constraints.Length.message"));
        assert!(!is_provider_key("com.acme.order.name.message"));
        assert!(!is_provider_key("jakarta.validation.constraints.Nonsense.message"));
    }

    #[test]
    fn the_startup_failures_are_reported_and_nothing_else_is() {
        let not_blank = constraint("NotBlank").unwrap();
        assert_eq!(verdict(not_blank, "String"), Verdict::Fits);
        assert!(matches!(verdict(not_blank, "int"), Verdict::Cannot(_)));
        assert!(matches!(verdict(not_blank, "List<String>"), Verdict::Cannot(_)));
        // A project type: nothing certain, so nothing said.
        assert_eq!(verdict(not_blank, "OrderId"), Verdict::Unknown);
        assert_eq!(verdict(not_blank, "T"), Verdict::Unknown);
    }

    #[test]
    fn size_accepts_the_four_things_that_have_one() {
        let size = constraint("Size").unwrap();
        for t in ["String", "List<Order>", "Map<String, Integer>", "byte[]"] {
            assert_eq!(verdict(size, t), Verdict::Fits, "{t}");
        }
        assert!(matches!(verdict(size, "long"), Verdict::Cannot(_)));
    }

    /// Not a failure — a line that reads as a rule and enforces nothing.
    #[test]
    fn not_null_on_a_primitive_is_pointless_rather_than_wrong() {
        let not_null = constraint("NotNull").unwrap();
        assert!(matches!(verdict(not_null, "int"), Verdict::Pointless(_)));
        assert_eq!(verdict(not_null, "Integer"), Verdict::Fits);
        assert_eq!(verdict(not_null, "OrderId"), Verdict::Fits, "any object can be null");
    }

    #[test]
    fn a_temporal_constraint_knows_both_date_apis() {
        let past = constraint("Past").unwrap();
        assert_eq!(verdict(past, "LocalDate"), Verdict::Fits);
        assert_eq!(verdict(past, "Date"), Verdict::Fits);
        assert!(matches!(verdict(past, "String"), Verdict::Cannot(_)));
    }
}
