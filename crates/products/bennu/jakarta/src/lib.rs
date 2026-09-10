//! `bennu-jakarta` — Jakarta / Java Bean Validation, as an editor needs to understand it.
//!
//! ## The one thing this exists to say
//!
//! **A constraint's message is not read from your message bundle.**
//!
//! It is very natural to assume otherwise: the application has `messages.properties`, the
//! constraint has `message = "{order.name.required}"`, and the two obviously go together. They do
//! not. Bean Validation resolves a message key against **`ValidationMessages`** — that exact base
//! name, at the classpath root — plus the provider's own bundle inside the jar. A key that lives in
//! `messages_it.properties` and nowhere else is a key the validator cannot find, and what it does
//! then is render the key itself, braces and all, into the field the user is looking at.
//!
//! Which is why this reads bundles again rather than asking the tooling that already indexed them:
//! that one answers *does any bundle declare this*, which is true, and is the wrong question.
//!
//! ## And the wiring is regularly nowhere you would look
//!
//! ```java
//! new ResourceBundleMessageInterpolator(
//!     new PlatformResourceBundleLocator("jakarta-validator-bundle"))
//! ```
//!
//! A bundle base name, as a string literal, inside a `@Bean`. Not in `validation.xml`, not in
//! `application.properties` — in a method body, redirecting every custom constraint message in the
//! application. [`bundles`] goes and finds it, because "the configuration is not where you expect
//! it" is the shape of problem this whole engine is for.
//!
//! ## The distinction the checks rest on
//!
//! In a message, `{min}` on a `@Size` is the **constraint's own attribute**, `${validatedValue}` is
//! an expression, and only `{order.name.length}` is a bundle key. A check that read every brace run
//! as a key would report two missing keys on every custom `@Size` message in the project — which is
//! why [`constraints::Constraint::attributes`] is data the checks consult rather than documentation
//! for a reader.
//!
//! ## What it will not claim
//!
//! [`constraints::verdict`] answers `Unknown` for every type it is not certain about — a project
//! type, a type variable, a JDK class outside a short list — so a custom `ConstraintValidator`,
//! which is invisible from here, is never contradicted. And nothing is reported about message keys
//! at all on a project with no validation bundle: that means the messages are literals or defaults,
//! not that every key in the project is wrong. Under-report rather than risk a false positive
//! (docs §7).

// What the constraints are, and what they can be put on.
pub mod constraints;
// Which bundle the validator actually reads — including one named in a `@Bean`.
pub mod bundles;
// The extension itself.
pub mod ext;
pub mod prelude;
// Finding constraints in a source, and reading their messages.
pub mod refs;
