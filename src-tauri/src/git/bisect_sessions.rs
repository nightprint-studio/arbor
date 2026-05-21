use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::process_ext::NoWindowExt;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BisectSessionStatus {
    Paused,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BisectSession {
    pub id: String,
    pub name: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub status: BisectSessionStatus,
    pub bad_hashes: Vec<String>,
    pub good_hashes: Vec<String>,
    pub result_hash: Option<String>,
    pub result_message: Option<String>,
}

// ── Path helpers ──────────────────────────────────────────────────────────────

fn bisect_root(repo_path: &str) -> PathBuf {
    PathBuf::from(repo_path).join(".arbor").join("bisect")
}

fn session_dir(repo_path: &str, id: &str) -> PathBuf {
    bisect_root(repo_path).join(id)
}

fn session_file(repo_path: &str, id: &str) -> PathBuf {
    session_dir(repo_path, id).join("session.json")
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn generate_id(bad_hash: Option<&str>) -> String {
    let ts = now_ms();
    let short = bad_hash.and_then(|h| h.get(..7)).unwrap_or("unknown");
    format!("{ts}_{short}")
}

// ── Public API ────────────────────────────────────────────────────────────────

/// List all saved bisect sessions for a repo, sorted newest first.
pub fn list_sessions(repo_path: &str) -> Result<Vec<BisectSession>> {
    let root = bisect_root(repo_path);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut sessions: Vec<BisectSession> = std::fs::read_dir(&root)
        .map_err(AppError::Io)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().to_string();
            let f = session_file(repo_path, &id);
            let txt = std::fs::read_to_string(f).ok()?;
            serde_json::from_str(&txt).ok()
        })
        .collect();
    sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(sessions)
}

/// Save the current bisect state as a paused session, then reset git bisect.
/// Returns the saved session.
pub fn save_and_pause(
    repo_path: &str,
    bad_hashes: Vec<String>,
    good_hashes: Vec<String>,
    name: Option<String>,
) -> Result<BisectSession> {
    let id = generate_id(bad_hashes.first().map(|s| s.as_str()));
    let now = now_ms();
    let auto_name = format!(
        "Session {}",
        bad_hashes.first().and_then(|h| h.get(..7)).unwrap_or("?")
    );
    let session = BisectSession {
        id: id.clone(),
        name: name.unwrap_or(auto_name),
        created_at: now,
        updated_at: now,
        status: BisectSessionStatus::Paused,
        bad_hashes,
        good_hashes,
        result_hash: None,
        result_message: None,
    };
    write_session(repo_path, &session)?;
    // Reset git bisect so the user can do other work
    let _ = crate::git_cli::command()
        .args(["bisect", "reset"])
        .current_dir(repo_path)
        .no_window()
        .output();
    Ok(session)
}

/// Auto-save a completed bisect session (result found). Does NOT reset git bisect.
pub fn save_result(
    repo_path: &str,
    bad_hashes: Vec<String>,
    good_hashes: Vec<String>,
    result_hash: String,
    result_message: Option<String>,
) -> Result<BisectSession> {
    let id = generate_id(Some(&result_hash));
    let now = now_ms();
    let short = result_hash.get(..7).unwrap_or(&result_hash);
    let msg_preview = result_message.as_deref().unwrap_or("").chars().take(40).collect::<String>();
    let name = if msg_preview.is_empty() {
        format!("Found: {short}")
    } else {
        format!("Found: {short} — {msg_preview}")
    };
    let session = BisectSession {
        id: id.clone(),
        name,
        created_at: now,
        updated_at: now,
        status: BisectSessionStatus::Completed,
        bad_hashes,
        good_hashes,
        result_hash: Some(result_hash),
        result_message,
    };
    write_session(repo_path, &session)?;
    Ok(session)
}

/// Resume a paused session by replaying its marks.
/// Returns the resulting BisectState.
pub fn resume_session(
    repo_path: &str,
    session_id: &str,
) -> Result<super::bisect::BisectState> {
    let f = session_file(repo_path, session_id);
    let txt = std::fs::read_to_string(&f).map_err(AppError::Io)?;
    let session: BisectSession = serde_json::from_str(&txt)
        .map_err(|e| AppError::Other(format!("bisect session parse error: {e}")))?;

    // Reset any existing bisect session first
    let _ = crate::git_cli::command()
        .args(["bisect", "reset"])
        .current_dir(repo_path)
        .no_window()
        .output();

    // Start a fresh session in no-checkout mode
    run_git(repo_path, &["bisect", "start", "--no-checkout"])?;

    // Apply bad marks first, then good marks
    for hash in &session.bad_hashes {
        run_git(repo_path, &["bisect", "bad", hash])?;
    }
    for hash in &session.good_hashes {
        // Ignore errors — a good commit might no longer exist
        let _ = crate::git_cli::command()
            .args(["bisect", "good", hash])
            .current_dir(repo_path)
            .no_window()
            .output();
    }

    super::bisect::get_bisect_state(repo_path)
}

/// Rename a saved session.
pub fn rename_session(repo_path: &str, session_id: &str, new_name: String) -> Result<BisectSession> {
    let f = session_file(repo_path, session_id);
    let txt = std::fs::read_to_string(&f).map_err(AppError::Io)?;
    let mut session: BisectSession = serde_json::from_str(&txt)
        .map_err(|e| AppError::Other(format!("bisect session parse error: {e}")))?;
    session.name = new_name;
    session.updated_at = now_ms();
    write_session(repo_path, &session)?;
    Ok(session)
}

/// Delete a saved session directory.
pub fn delete_session(repo_path: &str, session_id: &str) -> Result<()> {
    let dir = session_dir(repo_path, session_id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(AppError::Io)?;
    }
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn write_session(repo_path: &str, session: &BisectSession) -> Result<()> {
    let dir = session_dir(repo_path, &session.id);
    std::fs::create_dir_all(&dir).map_err(AppError::Io)?;
    let json = serde_json::to_string_pretty(session)
        .map_err(|e| AppError::Other(format!("bisect session serialize: {e}")))?;
    std::fs::write(session_file(repo_path, &session.id), json).map_err(AppError::Io)?;
    Ok(())
}

fn run_git(repo_path: &str, args: &[&str]) -> Result<()> {
    let out = crate::git_cli::command()
        .args(args)
        .current_dir(repo_path)
        .no_window()
        .output()
        .map_err(AppError::Io)?;
    if !out.status.success() {
        return Err(AppError::Other(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    Ok(())
}
