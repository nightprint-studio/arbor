//! `bennu-complete` — the parts of "offer a completion" that are not about the language.
//!
//! ## Why this crate exists
//!
//! The extension seam ([`bennu-ext`]) has always had the *transport* for completion:
//! `completions` returns a list, `inline_hint` returns a continuation. What it never had was
//! the **production** — and production is where the repetition is. Every provider that has
//! ever been written for bennu independently grew the same five steps:
//!
//! 1. find where the token under the caret starts, so the popup filters on the right prefix;
//! 2. decide which candidates that prefix admits;
//! 3. drop the ones already offered, because two vocabularies overlap;
//! 4. stop at some ceiling, because a two-letter prefix under a busy namespace is thousands;
//! 5. decide whether the answer is certain enough to be drawn *ahead* of the caret.
//!
//! None of those five is about Spring, or XML, or Java. The vocabulary is the provider's; the
//! mechanics are these.
//!
//! ## The rule worth centralising
//!
//! Step 5 is the reason this crate is worth its `Cargo.toml`. A completion popup may offer
//! twenty candidates and let the user choose — being wrong costs a keystroke. Ghost text is
//! rendered *inline, ahead of the caret*, where it reads like text that is already there:
//! being wrong costs trust, and a provider that guesses once is a provider you stop reading.
//!
//! So [`unique_continuation`] is the whole discipline in one function, and it is deliberately
//! strict: an empty prefix never ghosts (that would be inventing rather than continuing), and
//! two candidates that continue differently produce nothing. Two candidates that continue
//! *identically* do produce it — they are the same string, and refusing there would be
//! superstition rather than caution.
//!
//! ## What is not here
//!
//! Anything that needs to know what the text means. Where a key ends and a value begins, which
//! element may nest in which, whether the caret is inside a comment — that is the provider's
//! job, and pretending otherwise would produce a "generic" caret model that fits nothing. This
//! crate takes a prefix and gives back a disciplined answer.
//!
//! [`bennu-ext`]: https://docs.rs/bennu-ext
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_complete::prelude::...`.

// Line and token mechanics around a byte offset.
pub mod caret;
// Collecting candidates: de-duplicated, capped, in the order the provider offered them.
pub mod collect;
pub mod prefix;
pub mod prelude;
