//! `bennu-jsp` — the JSP model (**Phase-0 skeleton**).
//!
//! Role (docs §2, §4): a **homegrown JSP grammar** (community JSP grammars are dead —
//! docs §4), small on purpose: directives / scriptlets / expressions / EL, with Java
//! + HTML injection. Plus the TLD / taglib model and an EL / OGNL AST — the *resolver*
//! (OGNL value-stack, best-effort — docs §7) is a later phase, this crate carries the
//! parse + model.
//!
//! **Skeleton only**: role + prelude. The grammar, TLD model and EL/OGNL AST land in
//! the JSP-highlight (Phase 0) and config-graph (Phase 2) waves.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_jsp::prelude::...`.

pub mod prelude;
