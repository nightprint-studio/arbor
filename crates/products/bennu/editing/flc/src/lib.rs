//! `bennu-flc` — full-line completion: what the editor draws in grey after the caret.
//!
//! ## What is in here
//!
//! * [`tokenizer`] — the byte-level BPE the model was trained with. Training data and inference share it, so the
//!   model never meets a token it was not trained on.
//! * [`prompt`] — `[context header] PRE prefix SUF suffix MID`: the shape the model reads. The header is the host's:
//!   it lists the members of the types the code is about, which only the host's index can say.
//! * [`model`] — the two ONNX graphs (prefill, decode with a key/value cache) and the token-level search over them.
//!   Behind the `onnx` feature.
//! * [`comments`] — commented-out code, taken out of what the model reads when the user asks for it.
//! * [`constraint`] — the decoding rule that stops the model inventing a member the receiver does not have.
//! * [`freq`] — the project's own repeated lines, so a start several files finish the same way is completed from them.
//! * [`guard`] — where the model must stay silent, and the check that its line names nothing nobody has.
//! * [`header`] — the context block of the prompt: members of the types the code is about, in the trained shape.
//! * [`logs`] — the project's log statements as **counters per pattern**, never as a list of every call: what
//!   level, what shape, what message template, in which context — built once in the background and kept current
//!   by adding or removing one file's contribution at a time.
//!
//! ## What is not
//!
//! The index, the editor and the settings. This crate knows no `Java`, no project and no file system layout: it takes
//! text in and gives text and a confidence back, which is also what makes the logic testable without a model.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through `bennu_flc::prelude::...`.

pub mod comments;
pub mod constraint;
pub mod freq;
pub mod guard;
pub mod header;
pub mod logs;
pub mod mapper;
pub mod model;
pub mod prelude;
pub mod prompt;
pub mod tokenizer;
