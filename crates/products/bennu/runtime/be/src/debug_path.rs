//! What a watch **path** is: `order`, `order.customer.name`, `items[2].price`, `*head`.
//!
//! The grammar lives in `bennu-debug-expr` (`path`), next to the expression parser it is the strict
//! subset of; this module re-exports the pieces the debuggers walk with. A path is what mutation,
//! logpoints and the assistant's read-only tools accept — the wider expression language
//! (arithmetic, comparisons, statics) is [`crate::debug_jdwp_target`] and
//! [`crate::debug_dap_target`] over the shared evaluator.

pub(crate) use bennu_debug_expr::prelude::{parse_path as parse, Step, JAVA, RUST};
