//! `bennu-spel` — the two little languages that live inside Spring annotation strings.
//!
//! A Spring codebase writes expressions in places Java's own syntax has nothing to say
//! about: `@Value("${app.timeout:30}")`, `@ConditionalOnExpression("#{systemProperties['os']}")`,
//! `<property value="${db.url}"/>`. To an editor they are opaque string literals — which
//! is why a typo in one is invisible until the context fails to start.
//!
//! Two grammars, both handled here:
//!
//! - **Property placeholders** — `${key}` / `${key:default}`, nestable
//!   (`${a.${b}.c}`). See [`placeholder`].
//! - **SpEL** — `#{ … }`, the Spring Expression Language. Tokenized, not evaluated:
//!   we care about *where* each bean reference / property / literal is, so the editor
//!   can colour it, navigate from it, and flag the handful of things that are
//!   unambiguously broken. See [`spel`].
//!
//! ## Never a false positive
//!
//! Both parsers report issues, and both are deliberately partial about it. A missing
//! closing brace or an unterminated string is a fact about the text; "this operator
//! looks wrong" is an opinion, and opinions do not belong in a squiggle. The issue
//! lists here only carry the former (docs §7, the project-wide stance).
//!
//! ## Offsets
//!
//! Every span is a **byte** offset into the string that was passed in, half-open
//! `[start, end)`. The delimiters that drive both scanners (`$`, `#`, `{`, `}`, `:`,
//! quotes) are ASCII, so the byte scan never lands mid-codepoint — the spans are always
//! valid `str` slice bounds.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_spel::prelude::...`.

pub mod placeholder;
pub mod prelude;
pub mod spel;
