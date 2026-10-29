//! The real [`World`] a scenario runs in: this project's processes, recipes, HTTP and timeline.
//!
//! Also where starting a recipe is checked against the approvals — in one place, so a scenario step,
//! the assistant's `run_recipe` and the panel's Run button cannot disagree about what needs asking.

use std::path::Path;
use std::sync::Arc;

use bennu_live::prelude::{now_ms, Event, Filter, HttpRequest, HttpResponse, ProcInfo, Recipe, World};

use super::files::{read_recipes, Recipes};
use super::{approvals, conversation_key, key_of, owner_label, Live};

/// Whether the recipes file with `hash` may run for `owner`: the user approved it in the Live panel
/// (project-wide), or this conversation was asked and said yes. Another conversation's yes does not count.
pub(crate) fn recipes_approved(root: &str, owner: Option<&str>, hash: &str) -> bool {
    approvals().is_recipe_approved(&key_of(root), hash)
        || owner.is_some_and(|c| approvals().is_recipe_approved(&conversation_key(root, c), hash))
}

/// Record a yes for the recipes file: project-wide when the user gave it in the panel, for the one
/// conversation when it was given in a chat.
pub(crate) fn approve_recipes_for(root: &str, owner: Option<&str>, hash: &str) {
    let key = match owner {
        Some(c) => conversation_key(root, c),
        None => key_of(root),
    };
    approvals().approve_recipes(&key, hash);
}

/// The sentence that says a name or a port is taken, and by whom.
pub(crate) fn held_text(holder: &ProcInfo, what: &str) -> String {
    format!(
        "{what} is held by {}: `{}` ({}) is running. It was not duplicated and not stopped. You can read it (run_status, run_logs, timeline, http_request) but not stop or restart it; use another name or port, or wait until its owner stops it.",
        owner_label(holder.owner.as_deref()),
        holder.name,
        holder.id
    )
}

/// The services of `recipe` whose name or port a **different owner** already holds.
pub(crate) fn foreign_holders(live: &Live, recipe: &Recipe, owner: Option<&str>) -> Vec<(String, ProcInfo)> {
    let mut out: Vec<(String, ProcInfo)> = Vec::new();
    for s in recipe.start_order() {
        let by_name = live.manager.holder_of_name(&s.name);
        let by_port = s.ports.iter().find_map(|p| live.manager.holder_of_port(*p));
        if let Some(h) = by_name.or(by_port).filter(|h| h.owner.as_deref() != owner) {
            out.push((s.name.clone(), h));
        }
    }
    out
}

/// The recipe `name` of a project, only if its file is valid and the user approved that exact file.
pub(crate) fn approved_recipe(root: &str, name: &str, owner: Option<&str>) -> Result<Recipe, String> {
    let file = match read_recipes(root) {
        Recipes::None => return Err("this project has no recipes file (.arbor/bennu/live/recipes.toml)".into()),
        Recipes::Invalid { errors, .. } => return Err(format!("the recipes file is invalid:\n- {}", errors.join("\n- "))),
        Recipes::Valid(file) => file,
    };
    let recipe = file.recipe(name).ok_or_else(|| format!("no recipe `{name}` (known: {})", file.recipes.iter().map(|r| r.name.as_str()).collect::<Vec<_>>().join(", ")))?;
    if !recipes_approved(root, owner, &file.hash) {
        return Err("the recipes file has not been approved (or it changed since): the user must approve running its commands".into());
    }
    Ok(recipe.clone())
}

/// Start a recipe's services in order and wait for each. The shared body of every way to run one.
/// `owner` is the conversation running it (`None`: the user, from the panel). A service whose name or
/// port another owner holds is not started, adopted or killed: the whole call is refused up front.
pub(crate) fn run_recipe(live: &Live, name: &str, owner: Option<&str>, cancelled: &dyn Fn() -> bool) -> Result<Vec<ProcInfo>, String> {
    let recipe = approved_recipe(&live.root, name, owner)?;
    if let Some((service, holder)) = foreign_holders(live, &recipe, owner).into_iter().next() {
        return Err(held_text(&holder, &format!("service `{service}` of recipe `{name}`")));
    }
    live.manager.start_recipe(&recipe, Path::new(&live.root), owner, cancelled)
}

/// Stop a recipe's services in its stop order. With an `owner` only that owner's are stopped.
pub(crate) fn stop_recipe(live: &Live, name: &str, owner: Option<Option<&str>>) -> Result<Vec<ProcInfo>, String> {
    match read_recipes(&live.root) {
        Recipes::Valid(file) => {
            let recipe = file.recipe(name).ok_or_else(|| format!("no recipe `{name}`"))?;
            Ok(match owner {
                Some(owner) => live.manager.stop_recipe_of(recipe, owner),
                None => live.manager.stop_recipe(recipe),
            })
        }
        _ => Err("this project has no valid recipes file".into()),
    }
}

pub(crate) struct LiveWorld<'a> {
    pub live: Arc<Live>,
    pub cancelled: &'a dyn Fn() -> bool,
    /// Whose scenario this is: a conversation id, or `None` for the user's panel. A scenario starts and
    /// stops only what its owner may.
    pub owner: Option<String>,
}

fn describe(infos: &[ProcInfo]) -> String {
    infos.iter().map(|p| format!("{} [{}]", p.name, if p.ready { "ready" } else { "running" })).collect::<Vec<_>>().join(", ")
}

impl World for LiveWorld<'_> {
    fn start_recipe(&mut self, name: &str) -> Result<String, String> {
        run_recipe(&self.live, name, self.owner.as_deref(), self.cancelled).map(|i| format!("started {}", describe(&i)))
    }

    fn stop_recipe(&mut self, name: &str) -> Result<String, String> {
        stop_recipe(&self.live, name, Some(self.owner.as_deref())).map(|i| format!("stopped {} service(s)", i.len()))
    }

    fn stop_all(&mut self) -> String {
        // Only the scenario owner's processes: a scenario of one chat must not end another chat's services.
        format!("stopped {} process(es)", self.live.manager.stop_owned(self.owner.as_deref()))
    }

    fn http(&mut self, req: &HttpRequest) -> Result<HttpResponse, String> {
        // The runner already put a correlation id on the request; `Live::http` keeps it.
        self.live.http(req.clone()).map(|(resp, _)| resp)
    }

    fn sleep(&mut self, ms: u64) {
        // In slices, so a cancelled run does not sleep out a long wait.
        let mut left = ms;
        while left > 0 && !(self.cancelled)() {
            let step = left.min(100);
            std::thread::sleep(std::time::Duration::from_millis(step));
            left -= step;
        }
    }

    fn process_running(&self, name: &str) -> Option<bool> {
        self.live.manager.get(name).ok().map(|p| !p.phase.is_finished())
    }

    fn events(&self, filter: &Filter) -> Vec<Event> {
        self.live.timeline.query(filter)
    }

    fn now_ms(&self) -> u64 {
        now_ms()
    }

    fn cancelled(&self) -> bool {
        (self.cancelled)()
    }
}
