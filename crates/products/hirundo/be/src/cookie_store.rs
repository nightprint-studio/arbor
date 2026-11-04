//! The cookie jars: one per environment of a project, kept in the profile (never in the project's files — a
//! session cookie is a credential). The matching, expiry and `Set-Cookie` reading are `hirundo-model::cookies`;
//! this is where a jar is loaded for a send, written after one, and listed or edited by the window.

use hirundo_core::prelude::HirundoState;
use hirundo_model::prelude::{Cookie, Jar, Prepared};

use crate::project_state::{load, save};

fn file(env_id: &str) -> String {
    // The environment id is already a file-name-safe slug; "none" stands for requests sent with no environment.
    format!("cookies-{}", if env_id.is_empty() { "none" } else { env_id })
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn load_jar(project: &str, env_id: &str) -> Jar {
    let mut jar: Jar = load(project, &file(env_id));
    jar.purge(now_ms());
    jar
}

/// Put the jar's `Cookie` header on a request about to go out — unless the request names one itself.
pub fn attach(project: &str, env_id: &str, prepared: &mut Prepared) {
    if prepared.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("cookie")) {
        return;
    }
    if let Some(value) = load_jar(project, env_id).header_for(&prepared.url, now_ms()) {
        prepared.headers.push(("Cookie".into(), value));
    }
}

/// Keep what an answer's `Set-Cookie` headers say. A jar that cannot be written is not worth failing a send for.
pub fn keep(project: &str, env_id: &str, url: &str, response_headers: &[(String, String)]) {
    let set: Vec<&str> = response_headers.iter().filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie")).map(|(_, v)| v.as_str()).collect();
    if set.is_empty() {
        return;
    }
    let mut jar = load_jar(project, env_id);
    jar.store_all(set, url, now_ms());
    let _ = save(project, &file(env_id), &jar);
}

/// The cookies of an environment (`env_id` empty: requests sent with none).
#[arbor_rpc::handler]
fn cookies_list(_state: &HirundoState, project: String, env_id: String) -> Result<Vec<Cookie>, String> {
    Ok(load_jar(&project, &env_id).cookies)
}

/// Add or replace a cookie (the editor in the panel).
#[arbor_rpc::handler]
fn cookie_save(_state: &HirundoState, project: String, env_id: String, cookie: Cookie) -> Result<(), String> {
    if cookie.name.trim().is_empty() || cookie.domain.trim().is_empty() {
        return Err("a cookie needs a name and a domain".into());
    }
    let mut jar = load_jar(&project, &env_id);
    jar.store(cookie, now_ms());
    save(&project, &file(&env_id), &jar)
}

#[arbor_rpc::handler]
fn cookie_delete(_state: &HirundoState, project: String, env_id: String, name: String, domain: String, path: String) -> Result<(), String> {
    let mut jar = load_jar(&project, &env_id);
    jar.cookies.retain(|c| !(c.name == name && c.domain == domain && c.path == path));
    save(&project, &file(&env_id), &jar)
}

#[arbor_rpc::handler]
fn cookies_clear(_state: &HirundoState, project: String, env_id: String) -> Result<(), String> {
    save(&project, &file(&env_id), &Jar::default())
}

/// Write a text to a file the user chose (the event log of a stream).
#[arbor_rpc::handler]
fn text_save(_state: &HirundoState, file: String, text: String) -> Result<(), String> {
    if file.trim().is_empty() {
        return Err("choose a file".into());
    }
    std::fs::write(&file, text).map_err(|e| format!("could not write {file}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_environment_has_its_own_jar_file() {
        assert_eq!(file(""), "cookies-none");
        assert_eq!(file("staging"), "cookies-staging");
        assert_ne!(file("staging"), file("prod"));
    }
}
