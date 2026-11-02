//! The offline bench behind `docs/bennu-ml-bench.md`: three measures on PUBLIC code, one ignored test each.
//! Test-only — nothing here ships and nothing here changes how Bennu behaves.

mod cache_check;
mod common;
mod m1_cascade;
mod m2_synth;
mod m3_cache;
mod m3_misses;
mod m4_body;
mod m4_fill;
mod m4_methods;
mod m5_ngram;
mod m6_embed;
mod m9_grammar;
mod m9b_expr;
mod m9c_block;
mod m7_next;
mod m7_prefix;
mod m8_portions;
mod m78_shared;
mod perf_identity;
mod repro;
mod m3_rank;
mod m3_trace;
mod sites;
