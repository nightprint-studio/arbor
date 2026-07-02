# bennu-jsp

The Bennu **JSP model** — **Phase-0 skeleton**.

Role (docs §2, §4): a homegrown JSP grammar (community JSP grammars are dead — docs
§4), small on purpose: directives / scriptlets / expressions / EL, with Java + HTML
injection. Plus the TLD / taglib model and an EL / OGNL AST. The OGNL value-stack
*resolver* (best-effort — docs §7) is a later phase; this crate carries parse +
model.

Skeleton only: role + prelude. The grammar / TLD model / EL AST land in the
JSP-highlight (Phase 0) and config-graph (Phase 2) waves.
