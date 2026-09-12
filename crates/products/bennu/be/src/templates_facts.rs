//! What every code template reads about where it is used — `project` and `style` — for a project root.
//!
//! The shape and the rules are `bennu-templates`' ([`bennu_templates::facts`], [`bennu_templates::requires`]).
//! This is where the answers come from: the Java level the index read, the classpath Maven resolved (the
//! same coordinates the capabilities are recognised by, so "has Lombok" means one thing everywhere), and
//! Settings › Java Style.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use bennu_templates::prelude::{NamingFacts, ProjectFacts, StyleFacts, TemplateFacts};

use crate::index_service::IndexService;

/// How long an answer is reused. Abbreviations ask on keystrokes, and a classpath or a setting that changed
/// a moment ago is not worth reading the local repository again for each of them.
const FRESH: Duration = Duration::from_secs(3);

/// `project` and `style` for templates rendered in the project at `root`.
pub(crate) fn facts_for(root: &str) -> TemplateFacts {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, TemplateFacts)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some((at, facts)) = cache.lock().ok().and_then(|c| c.get(root).cloned()) {
        if at.elapsed() < FRESH {
            return facts;
        }
    }
    let facts = read(root);
    if let Ok(mut c) = cache.lock() {
        c.insert(root.to_string(), (Instant::now(), facts.clone()));
    }
    facts
}

fn read(root: &str) -> TemplateFacts {
    let java = IndexService::global().jdk_version_of(root).map(|v| crate::dtolab::java_major(&v)).unwrap_or(0);
    let coordinates = crate::capabilities::resolved_dependencies(Path::new(root));
    let resolved = !coordinates.is_empty();
    let project = ProjectFacts::new(java, coordinates, resolved);
    let config = bennu_core::config::load();
    let style = StyleFacts {
        final_params: config.java_final_params,
        lombok_val: config.java_lombok_val && project.version_of("org.projectlombok:lombok").is_some(),
        // A level not known yet (`0`) is not 10: `var` in a Java 8 project is a compile error, a type is not.
        local_var: config.java_local_var && project.java >= 10,
        switch_with_return: config.java_switch_with_return,
        space_in_braces: config.java_space_in_braces,
        blank_line_between_members: config.java_blank_line_between_members,
    };
    TemplateFacts { project, style, naming: NamingFacts::default() }
}

/// [`facts_for`] the project, with how it names things where `file` goes — the output's path, since a project
/// can name its tests differently from the rest of its code.
pub(crate) fn facts_at(root: &str, file: &str) -> TemplateFacts {
    TemplateFacts { naming: crate::naming::template_naming(root, file), ..facts_for(root) }
}
