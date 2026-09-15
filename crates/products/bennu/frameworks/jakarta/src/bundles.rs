//! Which bundle a constraint message is **actually** resolved against.
//!
//! ## The question, and why it is not the obvious one
//!
//! A project has message bundles. It also has constraint messages. It is very natural to assume
//! the second are read from the first, and on most projects that assumption is wrong: Bean
//! Validation does not know about your `messages.properties`. The spec says a `{key}` is resolved
//! against **`ValidationMessages`** — that exact base name, at the classpath root — and against the
//! provider's own bundle for the built-in defaults. Nothing else, unless somebody wired something
//! else.
//!
//! So a key that exists in `messages_it.properties` and nowhere else is a key the validator cannot
//! find, and what the user is shown is the key itself, in braces, in a form field. The general
//! message-bundle tooling answers *yes, some bundle declares it* — which is true, and is the wrong
//! answer to this question. That is the whole reason this crate reads bundles again rather than
//! asking the one that already indexed them.
//!
//! ## And the wiring is regularly in a `@Bean`
//!
//! ```java
//! new ResourceBundleMessageInterpolator(
//!     new PlatformResourceBundleLocator("jakarta-validator-bundle"))
//! ```
//!
//! That is a bundle base name written as a **string literal inside a method body**. It is in no
//! configuration file, no `validation.xml`, no `application.properties` — there is nothing to grep
//! for unless you already know what you are looking for, and it silently redirects every custom
//! constraint message in the application. Finding it is exactly the kind of thing this engine
//! exists for, so the Java sources are read for it.
//!
//! Spring has a third answer again: `LocalValidatorFactoryBean.setValidationMessageSource(…)` makes
//! the validator read Spring's own `MessageSource`, which usually *is* `messages`. That is recorded
//! as its own origin, because on such a project the naive assumption above becomes true and a check
//! that insisted on `ValidationMessages` would be the one crying wolf.

use bennu_i18n::prelude::Bundle;

/// Where a bundle came to be part of the validator's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// `ValidationMessages` — the base name the specification fixes. Nothing declares it; being
    /// called that IS the declaration.
    Spec,
    /// Named in Java, by a `PlatformResourceBundleLocator` or an `AggregateResourceBundleLocator`.
    Custom,
    /// Spring's `LocalValidatorFactoryBean` was pointed at the application's own `MessageSource`,
    /// so the ordinary message bundles answer constraint messages too.
    SpringMessageSource,
}

impl Origin {
    pub fn label(&self) -> &'static str {
        match self {
            Origin::Spec => "spec",
            Origin::Custom => "custom",
            Origin::SpringMessageSource => "Spring MessageSource",
        }
    }
}

/// Where something was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    /// Absolute path, forward-slashed.
    pub file: String,
    /// Byte offset of the thing itself — the string literal, the call.
    pub offset: usize,
    /// 1-based line.
    pub line: u32,
}

/// One bundle the validator reads, and the files that implement it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationBundle {
    /// Base name, without locale or extension (`ValidationMessages`, `jakarta-validator-bundle`).
    pub base: String,
    pub origin: Origin,
    /// The `.properties` files in the project that implement it, default locale first. **Empty is
    /// the interesting case**: a bundle named by a `@Bean` that nothing implements means every
    /// custom message in the application renders as its own key.
    pub files: Vec<String>,
    /// Where it was named, for the origins that are named somewhere.
    pub declared_in: Option<Site>,
}

/// What the validator will read, and what named it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Discovery {
    pub bundles: Vec<ValidationBundle>,
    /// Every key any of those bundles declares — the set a message key is checked against.
    pub keys: Vec<String>,
}

impl Discovery {
    /// Whether the validator can resolve `key`.
    ///
    /// The provider's own keys are **not** here — they live in the jar, and
    /// [`crate::constraints::is_provider_key`] answers for them. Kept apart on purpose: one is a
    /// fact about this project, the other a fact about the library, and merging them would make a
    /// project with no bundles at all look like one with twenty-nine keys.
    pub fn declares(&self, key: &str) -> bool {
        self.keys.iter().any(|k| k == key)
    }

    /// Whether anything was found at all. A project with no validation bundle is an ordinary
    /// project — every message is a literal, or every constraint uses its default — and no check
    /// here may fire on one.
    pub fn is_empty(&self) -> bool {
        self.bundles.is_empty()
    }

    /// The bundles that were named but that nothing in the project implements.
    pub fn unimplemented(&self) -> impl Iterator<Item = &ValidationBundle> {
        self.bundles.iter().filter(|b| b.files.is_empty() && b.origin != Origin::Spec)
    }
}

/// Read the project for the bundles Bean Validation will use.
///
/// `resources` are the `.properties` files (path, text); `java` the sources. Both are what the
/// host already scanned — nothing here touches the filesystem.
pub fn discover(resources: &[(String, String)], java: &[(String, String)]) -> Discovery {
    let parsed: Vec<Bundle> = resources
        .iter()
        .filter(|(p, _)| p.to_ascii_lowercase().ends_with(".properties"))
        .map(|(p, t)| Bundle::parse(p, t))
        .collect();

    let mut bundles: Vec<ValidationBundle> = Vec::new();

    // ── the base names something in the code asks for ────────────────────────
    for (path, text) in java {
        for (base, offset) in named_bundles(text) {
            if bundles.iter().any(|b| b.base == base) {
                continue;
            }
            bundles.push(ValidationBundle {
                files: files_for(&parsed, &base),
                base,
                origin: Origin::Custom,
                declared_in: Some(site(path, text, offset)),
            });
        }
        if let Some(offset) = spring_message_source(text) {
            if !bundles.iter().any(|b| b.origin == Origin::SpringMessageSource) {
                // Spring's own bundles, whatever they are called. `messages` is the default and
                // covers the overwhelming majority; a project that renamed it has said so in
                // configuration this does not read, and the honest thing is to take what is there.
                let base = spring_bundle_base(&parsed);
                bundles.push(ValidationBundle {
                    files: files_for(&parsed, &base),
                    base,
                    origin: Origin::SpringMessageSource,
                    declared_in: Some(site(path, text, offset)),
                });
            }
        }
    }

    // ── and the one the specification names ──────────────────────────────────
    let spec_files = files_for(&parsed, SPEC_BASE);
    if !spec_files.is_empty() {
        bundles.push(ValidationBundle {
            base: SPEC_BASE.to_string(),
            origin: Origin::Spec,
            files: spec_files,
            declared_in: None,
        });
    }

    // Spec first when it is there — it is the one that answers unless something overrode it, and a
    // list is read top down.
    bundles.sort_by_key(|b| match b.origin {
        Origin::Spec => 0,
        Origin::Custom => 1,
        Origin::SpringMessageSource => 2,
    });

    let mut keys: Vec<String> = bundles
        .iter()
        .flat_map(|b| b.files.iter())
        .filter_map(|path| parsed.iter().find(|p| &p.path == path))
        .flat_map(|b| b.entries.iter().map(|e| e.key.clone()))
        .collect();
    keys.sort();
    keys.dedup();

    Discovery { bundles, keys }
}

/// The base name the Bean Validation specification fixes.
pub const SPEC_BASE: &str = "ValidationMessages";

/// The `.properties` files implementing `base`, default locale first.
fn files_for(parsed: &[Bundle], base: &str) -> Vec<String> {
    let mut files: Vec<&Bundle> = parsed.iter().filter(|b| b.base == base).collect();
    files.sort_by(|a, b| a.locale.len().cmp(&b.locale.len()).then(a.path.cmp(&b.path)));
    files.into_iter().map(|b| b.path.clone()).collect()
}

/// Spring's message bundle, as the project actually spells it — `messages` unless there is no such
/// bundle and there is exactly one other candidate.
fn spring_bundle_base(parsed: &[Bundle]) -> String {
    if parsed.iter().any(|b| b.base == "messages") {
        return "messages".to_string();
    }
    let mut bases: Vec<&str> = parsed.iter().map(|b| b.base.as_str()).collect();
    bases.sort();
    bases.dedup();
    if bases.len() == 1 {
        return bases[0].to_string();
    }
    "messages".to_string()
}

/// The locator constructors that take a bundle base name.
const LOCATORS: [&str; 2] = ["PlatformResourceBundleLocator", "AggregateResourceBundleLocator"];

/// How far past a locator's opening paren to look for its string arguments. Long enough for an
/// `Arrays.asList("a", "b", "c")`, short enough that an unclosed paren cannot swallow the file.
const ARG_WINDOW: usize = 400;

/// Every bundle base name named by a locator in this source, with the offset of the literal.
fn named_bundles(source: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    for locator in LOCATORS {
        let mut from = 0usize;
        while let Some(at) = source[from..].find(locator) {
            let start = from + at + locator.len();
            from = start;
            // The name has to be followed by its argument list, allowing for whitespace — anything
            // else is the class being imported or mentioned, not called.
            let Some(open) = source[start..].find('(').filter(|i| {
                source[start..start + i].trim().is_empty()
            }) else {
                continue;
            };
            let args_at = start + open + 1;
            let window = &source[args_at..(args_at + ARG_WINDOW).min(source.len())];
            for (literal, offset) in string_literals(window) {
                if !literal.is_empty() {
                    out.push((literal, args_at + offset));
                }
            }
        }
    }
    out
}

/// Where Spring was told to answer constraint messages from its own `MessageSource`.
fn spring_message_source(source: &str) -> Option<usize> {
    source.find("setValidationMessageSource")
}

/// The string literals in a fragment, with the offset of each literal's text.
///
/// Deliberately simple: a bundle base name has never contained an escape, and the fragment being
/// read is an argument list.
fn string_literals(fragment: &str) -> Vec<(String, usize)> {
    let bytes = fragment.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut depth = 0i32;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            // Past the locator's own closing paren: whatever follows is a different call.
            b')' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            b'"' => {
                let start = i + 1;
                let mut j = start;
                while j < bytes.len() && bytes[j] != b'"' {
                    j += if bytes[j] == b'\\' { 2 } else { 1 };
                }
                if j > bytes.len() {
                    break;
                }
                if let Some(text) = fragment.get(start..j.min(fragment.len())) {
                    out.push((text.to_string(), start));
                }
                i = j;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

fn site(path: &str, text: &str, offset: usize) -> Site {
    Site {
        file: path.replace('\\', "/"),
        offset,
        line: text[..offset.min(text.len())].bytes().filter(|&b| b == b'\n').count() as u32 + 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reported shape, verbatim: the bundle name is a string literal inside a `@Bean`, and
    /// there is nothing else in the project that mentions it.
    const CONFIG: &str = r#"@Configuration
public class JakartaValidator {

    @Bean
    public Validator configure() {
        return Validation.byDefaultProvider()
                .configure()
                .messageInterpolator(
                    new ResourceBundleMessageInterpolator(
                        new PlatformResourceBundleLocator("jakarta-validator-bundle")
                    )
                ).buildValidatorFactory()
            .getValidator();
    }
}
"#;

    fn java(text: &str) -> Vec<(String, String)> {
        vec![("/p/src/main/java/JakartaValidator.java".to_string(), text.to_string())]
    }

    #[test]
    fn a_bundle_named_in_a_bean_is_found_and_matched_to_its_files() {
        let resources = vec![
            (
                "/p/src/main/resources/jakarta-validator-bundle.properties".to_string(),
                "order.name.required=Il nome è obbligatorio\n".to_string(),
            ),
            (
                "/p/src/main/resources/jakarta-validator-bundle_en.properties".to_string(),
                "order.name.required=The name is required\n".to_string(),
            ),
        ];
        let found = discover(&resources, &java(CONFIG));

        assert_eq!(found.bundles.len(), 1);
        let b = &found.bundles[0];
        assert_eq!(b.base, "jakarta-validator-bundle");
        assert_eq!(b.origin, Origin::Custom);
        assert_eq!(b.files.len(), 2, "both locales");
        assert!(b.files[0].ends_with("jakarta-validator-bundle.properties"), "default first");
        assert_eq!(b.declared_in.as_ref().unwrap().line, 10);
        assert!(found.declares("order.name.required"));
    }

    /// The check the whole discovery is for: a bundle the code asks for that nothing implements.
    /// Every custom message in the application then renders as its own key.
    #[test]
    fn a_named_bundle_with_no_files_is_the_thing_worth_reporting() {
        let found = discover(&[], &java(CONFIG));
        assert_eq!(found.bundles.len(), 1);
        assert!(found.bundles[0].files.is_empty());
        assert_eq!(found.unimplemented().count(), 1);
    }

    /// The spec's bundle is not declared anywhere — being called that is the declaration.
    #[test]
    fn the_specs_bundle_is_found_by_its_name_alone() {
        let resources = vec![(
            "/p/src/main/resources/ValidationMessages.properties".to_string(),
            "order.total.tooBig=Troppo\n".to_string(),
        )];
        let found = discover(&resources, &[]);
        assert_eq!(found.bundles.len(), 1);
        assert_eq!(found.bundles[0].origin, Origin::Spec);
        assert!(found.declares("order.total.tooBig"));
    }

    /// And the ordinary message bundle is NOT one of them — which is the assumption this crate
    /// exists to contradict.
    #[test]
    fn an_ordinary_message_bundle_is_not_a_validation_bundle() {
        let resources = vec![(
            "/p/src/main/resources/messages_it.properties".to_string(),
            "order.name.required=Obbligatorio\n".to_string(),
        )];
        let found = discover(&resources, &[]);
        assert!(found.is_empty());
        assert!(!found.declares("order.name.required"));
    }

    /// Unless Spring was told to use it, which is the one wiring that makes it true.
    #[test]
    fn spring_can_point_the_validator_at_the_applications_own_bundle() {
        let src = r#"@Bean
public LocalValidatorFactoryBean validator(MessageSource messages) {
    LocalValidatorFactoryBean bean = new LocalValidatorFactoryBean();
    bean.setValidationMessageSource(messages);
    return bean;
}"#;
        let resources = vec![(
            "/p/src/main/resources/messages.properties".to_string(),
            "order.name.required=Obbligatorio\n".to_string(),
        )];
        let found = discover(&resources, &java(src));
        assert_eq!(found.bundles.len(), 1);
        assert_eq!(found.bundles[0].origin, Origin::SpringMessageSource);
        assert_eq!(found.bundles[0].base, "messages");
        assert!(found.declares("order.name.required"));
    }

    #[test]
    fn several_names_in_one_aggregate_locator_are_all_found() {
        let src = r#"new ResourceBundleMessageInterpolator(
            new AggregateResourceBundleLocator(Arrays.asList("bundle-a", "bundle-b")));"#;
        let found = discover(&[], &java(src));
        let bases: Vec<&str> = found.bundles.iter().map(|b| b.base.as_str()).collect();
        assert_eq!(bases, ["bundle-a", "bundle-b"]);
    }

    /// An import is a mention, not a call — and a file that imports the locator without using it
    /// names no bundle.
    #[test]
    fn the_class_being_imported_is_not_a_call() {
        let src = "import org.hibernate.validator.resourceloading.PlatformResourceBundleLocator;\n";
        assert!(discover(&[], &java(src)).is_empty());
    }

    /// The literal scan must not run past the call it is reading.
    #[test]
    fn a_string_after_the_call_is_not_one_of_its_arguments() {
        let src = r#"new PlatformResourceBundleLocator("mine"); String other = "not-a-bundle";"#;
        let found = discover(&[], &java(src));
        let bases: Vec<&str> = found.bundles.iter().map(|b| b.base.as_str()).collect();
        assert_eq!(bases, ["mine"]);
    }
}
