//! `switch` rules: selectors, yields, duplicate and mistyped labels, enum exhaustiveness, fall-through smells.

pub mod enum_switch;
pub mod switch_dup;
pub mod switch_flow;
pub mod switch_label_type;
pub mod switches;
