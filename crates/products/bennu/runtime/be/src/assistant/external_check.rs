//! What is wrong with the external tools being edited in Settings › Imber — the same rules that decide whether
//! a run is offered one (`bennu_assistant::external_tools::problems`), so the page and the run cannot disagree.

use bennu_assistant::prelude::{tool_problems, ExternalTool};
use bennu_core::prelude::BennuState;

/// One list of problems per tool, in order; an empty list is a tool a run is offered (when it is switched on).
/// A program that is not a file on this machine is said too: it would only fail at the first call.
#[arbor_rpc::handler]
fn assistant_external_tool_problems(_ctx: &BennuState, tools: Vec<ExternalTool>) -> Result<Vec<Vec<String>>, String> {
    Ok(tools
        .iter()
        .map(|t| {
            let mut problems = tool_problems(t);
            let program = std::path::Path::new(t.program.trim());
            if problems.is_empty() && !program.is_file() {
                problems.push("The program was not found at this path.".to_string());
            }
            problems
        })
        .collect())
}
