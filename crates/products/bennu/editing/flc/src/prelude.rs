//! Canonical entry point for `bennu-flc`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through `bennu_flc::prelude::...`. The submodules
//! stay `pub` for rustdoc navigation, but the prelude is the canonical call-site path.

// Commented-out code, removed from the prompt on request.
pub use crate::comments::strip_commented_code;

// Decoding constraints: which bytes may come next.
pub use crate::constraint::{allowed_after, forced_prefix, names_constraint, ByteMask, Constraint};

// The project's repeated lines.
pub use crate::freq::{LineIndex, LineSuggestion};

// The context block of the prompt.
pub use crate::header::{build as build_header, build_in as header_in, mentioned_types, HeaderMember, TypeInfo, TypeLookup, HEADER_BUDGET};

// Where the model stays silent, and the check of what it wrote.
pub use crate::guard::{bare_type, continues_chain, cut_unbalanced, dots_after_prefix_close, gate, typed_word, vet, words_of, Gate};

// The project's log statements, as counters.
pub use crate::logs::{LogContext, LogProfile, LogSnapshot, LogSuggestion, Placement};

// The model and what it answers with.
pub use crate::model::{Candidate, LineModel, MAX_NEW_TOKENS};

// The prompt.
pub use crate::prompt::{Prompt, PREFIX_TOKENS, SUFFIX_TOKENS};

// The tokenizer.
pub use crate::tokenizer::{Tokenizer, EOT, MID, NEWLINE, PRE, SUF};

// Field-by-field mapping between two classes.
pub use crate::mapper::{next_mapping, read_context, render as render_mapping, stem as mapping_stem, Context as MappingContext, Pick as MappingPick, Prop as MappingProp, Style as MappingStyle, MIN_CONFIDENCE as MAPPING_MIN_CONFIDENCE};
