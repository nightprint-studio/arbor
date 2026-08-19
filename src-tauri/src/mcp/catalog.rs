//! The catalogue: what the AI surface actually exposes, and what happens when it is
//! called.
//!
//! This is the whole product half of MCP. `arbor-mcp` knows the protocol and nothing
//! else; everything that makes a tool call *Arbor's* — which backends are enabled, is
//! that backend even running, may this run, how big may the answer be, what does a
//! screenshot turn into — is decided here.
//!
//! ## The blocking rule
//!
//! Every path into a backend here goes through `spawn_blocking`. `ensure_*_be` and
//! `dispatch_rpc` both park on a synchronous channel, and a backend can call *back* into
//! the shell mid-dispatch (the reverse channel) with the shell answering via `block_on`.
//! Blocking a runtime worker on either would starve that path and deadlock — the same
//! landmine `open_bennu_window` documents. This is an HTTP handler on the same runtime,
//! so it is the same rule.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arbor_mcp::prelude::{CallToolResult, Content, Progress, Tool, ToolAnnotations, ToolCatalog};
use arbor_rpc::prelude::{Safety, ToolDescriptor, ToolOutput};
use async_trait::async_trait;
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::mcp::{audit, consent, policy};
use crate::AppState;

/// Which programs may be exposed, and how to bring each one up.
///
/// An explicit table rather than "every attached backend": adding a product to Arbor
/// must never be the same act as adding its tools to the AI surface.
const EXPOSABLE: &[(&str, fn(&AppHandle))] = &[
    ("bennu", crate::ipc::ensure_bennu_be),
    ("tyto", crate::ipc::ensure_tyto_be),
];

/// Whether a program can appear on the AI surface at all.
pub fn is_exposable(program: &str) -> bool {
    EXPOSABLE.iter().any(|(name, _)| *name == program)
}

/// A tool, plus where it came from.
#[derive(Clone)]
struct Routed {
    program: &'static str,
    descriptor: ToolDescriptor,
}

/// Arbor's implementation of the MCP catalogue.
pub struct ShellCatalog {
    app: AppHandle,
    /// Descriptors are stable for a backend's lifetime (they come from its compiled-in
    /// inventory), so they are fetched once per program and kept. Cleared when the
    /// config changes, because which programs are enabled can change.
    cache: Arc<Mutex<HashMap<String, Vec<Routed>>>>,
}

impl ShellCatalog {
    pub fn new(app: AppHandle) -> Self {
        Self { app, cache: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// Forget the cached descriptors — after a config change, or after a backend was
    /// recycled onto another profile.
    pub fn invalidate(&self) {
        if let Ok(mut c) = self.cache.lock() {
            c.clear();
        }
    }

    /// Every routed tool of every enabled program, bringing backends up as needed.
    ///
    /// Runs entirely on the blocking pool: `ensure_*_be` waits for the child's `Hello`.
    fn collect_blocking(app: &AppHandle, cache: &Mutex<HashMap<String, Vec<Routed>>>) -> Vec<Routed> {
        let enabled: Vec<(&'static str, fn(&AppHandle))> = {
            let state = app.state::<AppState>();
            let Ok(cfg) = state.lock_config() else { return Vec::new() };
            EXPOSABLE
                .iter()
                .filter(|(program, _)| cfg.mcp.products.get(*program).copied().unwrap_or(false))
                .copied()
                .collect()
        };

        let mut out = Vec::new();
        for (program, ensure) in enabled {
            out.extend(tools_of(app, Some(cache), program, ensure).unwrap_or_default());
        }
        out.sort_by(|a, b| a.descriptor.name.cmp(&b.descriptor.name));
        out
    }
}

/// One program's tools, from its own compiled-in inventory.
///
/// `Err` carries the reason, because the two ways this comes back empty are different
/// answers: a backend that would not start, and one that genuinely exposes nothing.
fn tools_of(
    app: &AppHandle,
    cache: Option<&Mutex<HashMap<String, Vec<Routed>>>>,
    program: &'static str,
    ensure: fn(&AppHandle),
) -> Result<Vec<Routed>, String> {
    if let Some(hit) = cache.and_then(|c| c.lock().ok().and_then(|c| c.get(program).cloned())) {
        return Ok(hit);
    }
    // Listing starts the backend. It is the only way to learn what it serves, and it
    // happens once per session rather than per call.
    ensure(app);
    let state = app.state::<AppState>();
    let value = crate::ipc::dispatch_rpc(state.inner(), program, arbor_be::TOOLS_METHOD, Value::Null)
        .map_err(|e| {
            // A backend that is not up, or one that predates the tool seam, contributes
            // nothing rather than failing the whole listing.
            tracing::debug!("mcp: {program} has no tool surface: {e}");
            format!("{program} did not answer: {e}")
        })?;
    let descriptors: Vec<ToolDescriptor> = serde_json::from_value(value).unwrap_or_default();
    let routed: Vec<Routed> =
        descriptors.into_iter().map(|descriptor| Routed { program, descriptor }).collect();
    if let Some(cache) = cache {
        if let Ok(mut c) = cache.lock() {
            c.insert(program.to_string(), routed.clone());
        }
    }
    Ok(routed)
}

/// One program's contribution to the AI surface, for a reader rather than for a client.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProgramTools {
    pub program: String,
    /// Whether the user currently exposes this program. Its tools are listed either way:
    /// "what would this let an assistant do" is the question you have *before* deciding
    /// to switch it on, and a list that appears only once you have is no help with it.
    pub exposed: bool,
    pub tools: Vec<ToolSummary>,
    /// Why there is nothing here, when there isn't.
    pub detail: Option<String>,
}

/// One tool, described for someone reading rather than calling.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolSummary {
    /// The name a client calls.
    pub name: String,
    /// The backend handler behind it — the same string an audit line reports. Differs
    /// from `name` wherever a method name was not unique or not legible across products.
    pub method: String,
    pub title: String,
    pub description: String,
    /// `read` | `write` | `destructive`.
    pub safety: String,
    pub idempotent: bool,
}

/// Everything Arbor can expose, program by program.
///
/// Blocks: it starts each backend to read its inventory. Call it off the async runtime —
/// see this module's header.
pub fn listing(app: &AppHandle) -> Vec<ProgramTools> {
    let enabled = |program: &str| {
        let state = app.state::<AppState>();
        state
            .lock_config()
            .map(|cfg| cfg.mcp.products.get(program).copied().unwrap_or(false))
            .unwrap_or(false)
    };

    EXPOSABLE
        .iter()
        .map(|(program, ensure)| {
            let (tools, detail) = match tools_of(app, None, program, *ensure) {
                Ok(routed) => {
                    let mut tools: Vec<ToolSummary> = routed
                        .into_iter()
                        .map(|r| ToolSummary {
                            name: r.descriptor.name,
                            method: r.descriptor.method,
                            title: r.descriptor.title,
                            description: r.descriptor.description,
                            safety: safety_label(r.descriptor.safety).to_string(),
                            idempotent: r.descriptor.idempotent,
                        })
                        .collect();
                    tools.sort_by(|a, b| a.name.cmp(&b.name));
                    let detail = tools.is_empty().then(|| {
                        format!("{program} is running but exposes no tools.")
                    });
                    (tools, detail)
                }
                Err(e) => (Vec::new(), Some(e)),
            };
            ProgramTools {
                program: program.to_string(),
                exposed: enabled(program),
                tools,
                detail,
            }
        })
        .collect()
}

#[async_trait]
impl ToolCatalog for ShellCatalog {
    async fn list(&self) -> Vec<Tool> {
        let app = self.app.clone();
        let cache = Arc::clone(&self.cache);
        let routed = tokio::task::spawn_blocking(move || Self::collect_blocking(&app, &cache))
            .await
            .unwrap_or_default();
        routed.into_iter().map(|r| to_mcp_tool(&r.descriptor)).collect()
    }

    async fn call(&self, name: &str, arguments: Value, progress: &Progress) -> CallToolResult {
        let app = self.app.clone();
        let cache = Arc::clone(&self.cache);
        let wanted = name.to_string();

        let found = {
            let app = app.clone();
            tokio::task::spawn_blocking(move || {
                Self::collect_blocking(&app, &cache)
                    .into_iter()
                    .find(|r| r.descriptor.name == wanted)
            })
            .await
            .unwrap_or(None)
        };

        let Some(routed) = found else {
            // Not an error the protocol should carry: the model can read this and pick
            // another tool.
            return CallToolResult::error(format!(
                "No tool named `{name}` is available. The set of tools depends on which \
                 Arbor products the user has enabled for AI access; call tools/list again \
                 to see the current set."
            ));
        };

        let safety = routed.descriptor.safety;
        let mut entry = audit::AuditEntry::opening(
            routed.descriptor.name.clone(),
            routed.program.to_string(),
            safety_label(safety).to_string(),
            audit::preview(&arguments),
        );
        // On the log before it is decided, not after it finishes. A call parked in front
        // of a consent prompt, or grinding through a two-minute test run, is exactly when
        // someone looks — and a log written only on completion is blank precisely then.
        audit::open(&app, &entry);
        entry.outcome = "denied".into();

        // ── Policy ──────────────────────────────────────────────────────────
        // On a blocking thread like everything else here. `decide` reads the config
        // mutex, which the rest of the shell holds across real work — parking an async
        // worker on it is the deadlock this file's header warns about, and the fact
        // that it happened to return quickly today is luck, not a guarantee.
        let verdict = {
            let app = app.clone();
            let program = routed.program.to_string();
            let tool = routed.descriptor.name.clone();
            let args = arguments.clone();
            tokio::task::spawn_blocking(move || {
                let state = app.state::<AppState>();
                policy::decide(state.inner(), &program, &tool, safety, &args)
            })
            .await
            .unwrap_or_else(|_| {
                // The decision panicked or was cancelled. Fail closed: a permission
                // check that did not finish has not said yes.
                policy::Verdict::Deny("Arbor could not complete its permission check.".into())
            })
        };
        match verdict {
            policy::Verdict::Deny(reason) => {
                entry.detail = Some(reason.clone());
                audit::record(&app, entry);
                return CallToolResult::error(reason);
            }
            policy::Verdict::Ask if !consent::granted_for_session(&routed.descriptor.name) => {
                let timeout = {
                    let state = app.state::<AppState>();
                    state
                        .lock_config()
                        .map(|c| c.mcp.consent_timeout_secs)
                        .unwrap_or(120)
                };
                let request = consent::ConsentRequest {
                    id: consent::new_id(),
                    tool: routed.descriptor.name.clone(),
                    title: routed.descriptor.title.clone(),
                    program: routed.program.to_string(),
                    safety: safety_label(safety).to_string(),
                    description: routed.descriptor.description.clone(),
                    arguments: serde_json::to_string_pretty(&arguments).unwrap_or_default(),
                };
                audit::asking(&app, entry.id);
                let allowed = consent::ask(&app, request, Duration::from_secs(timeout)).await;
                if !allowed {
                    entry.outcome = "asked_denied".into();
                    entry.detail = Some("The user did not approve this call.".into());
                    audit::record(&app, entry);
                    return CallToolResult::error(
                        "The user declined this call. Do not retry it; ask them what to do \
                         instead."
                            .to_string(),
                    );
                }
                entry.outcome = "asked_allowed".into();
            }
            _ => entry.outcome = "allowed".into(),
        }

        // ── Dispatch ────────────────────────────────────────────────────────
        let params = routed.descriptor.wrap_arguments(arguments);
        let program = routed.program;
        let method = routed.descriptor.method.clone();
        let app_call = app.clone();
        let started = Instant::now();
        audit::running(&app, entry.id);
        let mut task = tokio::task::spawn_blocking(move || {
            let state = app_call.state::<AppState>();
            crate::ipc::dispatch_rpc(state.inner(), program, &method, params)
        });
        // Narration is relayed for every call, not only for a client that asked to hear
        // it: the AI client is one audience and the person watching Arbor is the other,
        // and only the first one has to ask.
        let result = relay_progress(&app, program, entry.id, &mut task, progress).await;
        entry.duration_ms = Some(started.elapsed().as_millis() as u64);

        let max_bytes = {
            let state = app.state::<AppState>();
            state.lock_config().map(|c| c.mcp.max_result_bytes).unwrap_or(256 * 1024)
        };

        match result {
            Ok(Ok(value)) => {
                audit::record(&app, entry);
                render(&routed.descriptor, value, max_bytes)
            }
            Ok(Err(e)) => {
                // The backend's own error string, verbatim — it is the contract, and it
                // is usually the most actionable thing the model can be told.
                entry.outcome = "failed".into();
                entry.detail = Some(e.to_string());
                audit::record(&app, entry);
                CallToolResult::error(e.to_string())
            }
            Err(join) => {
                entry.outcome = "failed".into();
                entry.detail = Some(join.to_string());
                audit::record(&app, entry);
                CallToolResult::error(format!("Arbor failed to run the tool: {join}"))
            }
        }
    }
}

/// Await `task`, collecting `program`'s narration and forwarding it to the client.
///
/// Only the progress topic is subscribed to. Every other backend event is shaped for a UI
/// that holds state across them — a client reading "class result: {…}" forty times learns
/// nothing the final summary would not tell it once, and a test run emits thousands.
async fn relay_progress(
    app: &AppHandle,
    program: &'static str,
    entry_id: u64,
    task: &mut tokio::task::JoinHandle<Result<Value, crate::error::AppError>>,
    progress: &Progress,
) -> Result<Result<Value, crate::error::AppError>, tokio::task::JoinError> {
    let mut events = crate::ipc::event_tap::tap()
        .subscribe(program, arbor_ipc::prelude::PROGRESS_TOPIC);
    loop {
        tokio::select! {
            // Biased: when the call has finished and narration is still queued, the answer
            // is what the client is waiting for. The remaining lines describe work that is
            // already over.
            biased;
            done = &mut *task => return done,
            Some(event) = events.recv() => {
                let Some(message) = event.payload.get("message").and_then(Value::as_str) else {
                    continue;
                };
                let done = event.payload.get("done").and_then(Value::as_u64);
                let total = event.payload.get("total").and_then(Value::as_u64);
                audit::progress(app, entry_id, message);
                progress.send(message, done, total).await;
            }
        }
    }
}

/// An `arbor-rpc` descriptor as an MCP tool.
fn to_mcp_tool(d: &ToolDescriptor) -> Tool {
    Tool {
        name: d.name.clone(),
        title: Some(d.title.clone()),
        description: d.description.clone(),
        input_schema: d.input_schema.clone(),
        annotations: ToolAnnotations {
            read_only_hint: d.safety.read_only(),
            destructive_hint: d.safety.destructive(),
            idempotent_hint: d.idempotent,
            open_world_hint: d.open_world,
        },
    }
}

fn safety_label(safety: Safety) -> &'static str {
    match safety {
        Safety::Read => "read",
        Safety::Write => "write",
        Safety::Destructive => "destructive",
    }
}

/// Turn a handler's JSON result into content blocks, within budget.
fn render(descriptor: &ToolDescriptor, value: Value, max_bytes: usize) -> CallToolResult {
    match descriptor.output {
        ToolOutput::Image => match as_image(&value) {
            Some(content) => CallToolResult::blocks(vec![content]),
            // A handler that declared `output = image` and returned something else is a
            // bug in that handler — but degrading to JSON keeps the tool usable rather
            // than turning a mislabelled result into no result.
            None => CallToolResult::text(cap(value.to_string(), max_bytes)),
        },
        ToolOutput::Text => {
            let text = value.as_str().map(str::to_string).unwrap_or_else(|| value.to_string());
            CallToolResult::text(cap(text, max_bytes))
        }
        ToolOutput::Json => CallToolResult::text(cap(value.to_string(), max_bytes)),
    }
}

fn as_image(value: &Value) -> Option<Content> {
    let mime = value.get("mime_type")?.as_str()?;
    let data = value.get("data")?.as_str()?;
    Some(Content::image(data, mime))
}

/// Truncate, and say so.
///
/// Silence here is the failure mode that matters: a project tree cut at 256KB with no
/// note reads as a complete listing, and the model concludes a directory does not exist.
fn cap(text: String, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n\n[Arbor truncated this result: {} bytes of {}. Narrow the request — a smaller \
         depth, a more specific path, or a tighter query — rather than retrying the same call.]",
        &text[..end],
        end,
        text.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(output: ToolOutput) -> ToolDescriptor {
        ToolDescriptor {
            name: "t".into(),
            method: "t".into(),
            title: "T".into(),
            description: String::new(),
            input_schema: Value::Null,
            safety: Safety::Read,
            idempotent: true,
            open_world: false,
            wrap_in: None,
            output,
        }
    }

    #[test]
    fn only_the_listed_programs_can_reach_the_ai_surface() {
        // The guard on the respawn hook: attaching corvus must not tell a client the tool
        // set moved, because corvus contributes none of it.
        assert!(is_exposable("bennu"));
        assert!(!is_exposable("corvus"));
    }

    #[test]
    fn an_image_result_becomes_an_image_block() {
        let value = serde_json::json!({ "mime_type": "image/png", "data": "AAAA", "width": 10 });
        let out = render(&descriptor(ToolOutput::Image), value, 1024);
        assert!(matches!(out.content[0], Content::Image { .. }));
        assert!(!out.is_error);
    }

    #[test]
    fn a_mislabelled_image_degrades_to_json_rather_than_vanishing() {
        let value = serde_json::json!({ "not": "an image" });
        let out = render(&descriptor(ToolOutput::Image), value, 1024);
        assert!(matches!(out.content[0], Content::Text { .. }));
    }

    #[test]
    fn a_text_tool_is_not_re_quoted() {
        let out = render(&descriptor(ToolOutput::Text), serde_json::json!("hello"), 1024);
        match &out.content[0] {
            Content::Text { text } => assert_eq!(text, "hello"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn truncation_announces_itself() {
        let long = "x".repeat(5000);
        let out = cap(long, 100);
        assert!(out.contains("Arbor truncated this result"));
        assert!(out.contains("5000"));
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        let out = cap("é".repeat(500), 101);
        assert!(out.is_char_boundary(out.len()));
    }
}
