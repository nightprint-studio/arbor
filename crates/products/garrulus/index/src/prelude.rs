//! The crate's public surface. Consumers write `garrulus_index::prelude::Index`
//! (or `use garrulus_index::prelude::*;`) and never reach into the submodules.

pub use crate::fuzzy::{matches as fuzzy_matches, score as fuzzy_score, FuzzyMatch};
pub use crate::graph::{
    leaf_key, link_key, Backlink, Edge, LinkGraph, Mention, Resolver, UnresolvedLink,
};
pub use crate::index::{Hit, Index};
pub use crate::note_view::{
    flatten_frontmatter, front_value_to_string, note_id, type_id, LinkRef, NoteView,
};
pub use crate::problems::{collect as collect_problems, Problem, Severity};
pub use crate::query::{
    matches_filter, matches_filters, parse_query, Filter, FilterOp, Query, SortField, SortKey,
    SortOrder,
};
pub use crate::text::{
    find_ignore_case, snippet, tokenize, MatchRange, Snippet, TextIndex,
};
