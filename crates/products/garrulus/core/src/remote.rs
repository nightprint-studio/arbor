//! `remote` — what a vault's sync destination *is* on disk, and the one place
//! that turns it into a live [`SyncRemote`].
//!
//! ## Why the description lives in the registry and not in the vault
//!
//! [`RemoteConfig`] is the persisted shape of `VaultEntry::remote`
//! (`garrulus/vaults.json`), **not** of `<vault>/.arbor/garrulus/vault.toml`, and
//! that is deliberate (`docs/garrulus-design.md` §4.1):
//!
//! * a folder remote's mirror path is **machine-specific** — a USB mount point or
//!   a Drive folder — and would be wrong on the other PC the moment the vault
//!   synced it across;
//! * for a git remote the URL already lives in `.git/config`, so writing it into
//!   a synced file only creates two records that can disagree.
//!
//! This mirrors corvus's `repos.json`, which is likewise the only place
//! machine-specific absolute paths are allowed to live.
//!
//! ## Why there is exactly one factory
//!
//! Four call sites need a `SyncRemote` from a `RemoteConfig`: opening a vault,
//! configuring a destination, testing one before persisting it, and creating a
//! remote repository. If each built its own, the device name, the daily-note
//! folder or the credential wiring would drift between "the remote you tested"
//! and "the remote you got". [`build_remote`] is the only constructor; everything
//! else goes through it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use corvus_git::prelude::GitCli;
use garrulus_sync::prelude::{
    CredentialProvider, FolderRemote, GitRemote, RemoteKind, SyncRemote,
};
use garrulus_vault::prelude::VaultConfig;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::state::GarrulusState;

/// The git remote a vault syncs against when none was named.
///
/// A vault Garrulus cloned or created has exactly one remote and it is called
/// `origin`; storing the name anyway keeps the odd hand-made vault (`backup`,
/// `home`) expressible without a second config shape.
pub const DEFAULT_GIT_REMOTE: &str = "origin";

/// A vault's configured sync destination, as persisted on its registry entry.
///
/// Flat rather than an enum-with-payload on purpose: the frontend edits these
/// fields in one form, and a flat record round-trips through a partially-filled
/// form without the "which variant is this JSON" ambiguity an internally tagged
/// enum introduces. The invariants an enum would have encoded are enforced by
/// [`validate`](Self::validate) instead — which produces a message the user can
/// act on, where a serde error would not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteConfig {
    /// Which implementation backs this destination.
    pub kind: RemoteKind,
    /// Git only: the remote's name. Absent means [`DEFAULT_GIT_REMOTE`].
    #[serde(default)]
    pub git_remote: Option<String>,
    /// Git only: the branch to track. Absent means "whatever is checked out",
    /// which is what a vault the user never branches always wants.
    #[serde(default)]
    pub branch: Option<String>,
    /// Folder only: the **absolute** path of the mirror directory. Machine-local
    /// by nature — this is the field that must never travel to the other PC.
    #[serde(default)]
    pub folder: Option<String>,
}

impl RemoteConfig {
    /// A git destination against the named remote (pass [`DEFAULT_GIT_REMOTE`]
    /// unless the user chose otherwise).
    pub fn git(remote: impl Into<String>) -> Self {
        Self {
            kind:       RemoteKind::Git,
            git_remote: Some(remote.into()),
            branch:     None,
            folder:     None,
        }
    }

    /// A folder destination mirroring the vault to `path` (absolute).
    pub fn folder(path: impl Into<String>) -> Self {
        Self {
            kind:       RemoteKind::Folder,
            git_remote: None,
            branch:     None,
            folder:     Some(path.into()),
        }
    }

    /// Track `branch` instead of whatever is checked out.
    pub fn with_branch(mut self, branch: Option<String>) -> Self {
        self.branch = branch;
        self
    }

    /// The git remote name to talk to, with the default already applied. Blank
    /// input counts as absent — an empty text field is a user who did not choose,
    /// not a remote whose name is the empty string.
    pub fn remote_name(&self) -> &str {
        match self.git_remote.as_deref().map(str::trim) {
            Some(name) if !name.is_empty() => name,
            _ => DEFAULT_GIT_REMOTE,
        }
    }

    /// The branch to track, or `None` for "the checked-out one". Blank counts as
    /// absent, for the same reason as [`remote_name`](Self::remote_name).
    pub fn branch_name(&self) -> Option<String> {
        self.branch
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .map(str::to_string)
    }

    /// The mirror directory of a folder destination.
    ///
    /// An error — never a silent fallback — when it is missing or relative. A
    /// missing mirror path would leave the vault local-only while the UI claimed
    /// it had a destination, and a *relative* one would resolve against
    /// `garrulus-be`'s working directory, which no user has ever chosen: both
    /// have to be rejected where the value is read, not discovered at the first
    /// sync.
    pub fn mirror_path(&self) -> Result<PathBuf, String> {
        if self.kind != RemoteKind::Folder {
            return Err("this is a git destination, not a folder one".to_string());
        }
        let raw = self.folder.as_deref().map(str::trim).unwrap_or_default();
        if raw.is_empty() {
            return Err("a folder destination needs the folder to mirror the vault to".to_string());
        }
        let path = PathBuf::from(raw);
        if !path.is_absolute() {
            return Err(format!(
                "the mirror folder must be an absolute path, not '{raw}'"
            ));
        }
        Ok(path)
    }

    /// Is this destination usable? Checked before persisting it, so a broken
    /// destination is refused where the user typed it rather than at the next
    /// sync, when the context is gone.
    pub fn validate(&self) -> Result<(), String> {
        match self.kind {
            RemoteKind::Git => Ok(()),
            RemoteKind::Folder => self.mirror_path().map(|_| ()),
        }
    }
}

/// Bind the git smart-HTTP credential lookup to the shell's reverse channel.
///
/// The keyring is shell-side and stays there: this marshals `(url) -> (user,
/// pass)` to the shell's `__git_credentials` broker, exactly as `corvus-be`'s
/// resolver does, preceded by the proactive `__maybe_refresh_url` so an expiring
/// OAuth token is renewed *before* it is read rather than after a push fails.
///
/// **Closes over the [`HostCaller`](arbor_ipc::prelude::HostCaller), not over the
/// state.** The remote this provider ends up inside is itself owned by
/// `GarrulusState`; capturing the state would close a reference cycle that keeps
/// it alive forever. The host caller has no way back to the state, so it does
/// not.
///
/// `host_call` blocks on the shell's reply — which is what the reverse channel is
/// built for — so this must never run while a lock guard is held.
pub fn credential_provider(state: &GarrulusState) -> CredentialProvider {
    let host = state.host_caller();
    Arc::new(move |url: &str| {
        let host = host.as_ref().ok_or_else(|| {
            "garrulus: no reverse channel to the credential broker".to_string()
        })?;
        // Best-effort by design: a refresh that fails must not stop us from
        // trying the credential we already have.
        let _ = host.call("__maybe_refresh_url", json!(url));
        let value = host.call("__git_credentials", json!(url))?;
        serde_json::from_value::<Option<(String, String)>>(value).map_err(|e| e.to_string())
    })
}

/// Turn a persisted [`RemoteConfig`] into the live remote for the vault at
/// `root` — the **only** place that mapping exists.
///
/// Everything machine-scoped is resolved here from the profile config (the
/// device name that authors every sync commit) and everything vault-scoped from
/// the vault's own file (the daily-note folder, so daily notes append-merge
/// instead of conflicting). A caller therefore cannot forget either.
///
/// Takes `&GarrulusState` rather than `&Arc<GarrulusState>` so it reads the same
/// from an RPC handler (which is handed a `&GarrulusState`) and from a background
/// worker holding an `Arc` — `&arc` coerces on the way in.
pub fn build_remote(
    state: &GarrulusState,
    root: &Path,
    config: &RemoteConfig,
) -> Result<Box<dyn SyncRemote>, String> {
    let device = crate::config::load().device_name;
    let daily = daily_folder(root);

    match config.kind {
        RemoteKind::Git => {
            let remote = GitRemote::new(
                root,
                config.remote_name(),
                device,
                git_cli(),
                credential_provider(state),
            )
            .with_branch(config.branch_name())
            .with_daily_folder(daily);
            Ok(Box::new(remote))
        }
        RemoteKind::Folder => {
            let mirror = config.mirror_path()?;
            let remote = FolderRemote::new(root, mirror, device).with_daily_folder(daily);
            Ok(Box::new(remote))
        }
    }
}

/// The git binary a [`GitRemote`] shells out to.
///
/// `garrulus-be` runs no git detection of its own (corvus's `git-cli` global is
/// filled by whoever calls `detect()`, and this process never does), so the
/// resolved program is plain `git` on `PATH` — which is the right answer on every
/// machine that has git installed normally. If Garrulus ever has to honour the
/// shell's configured / portable git, this function is the single place to read
/// it from, and nothing else changes.
fn git_cli() -> GitCli {
    GitCli::from_optional(None)
}

/// The vault's daily-note folder, if the vault has been set up.
///
/// Best-effort on purpose: a vault with no `vault.toml` yet is an ordinary state,
/// and a malformed one must not stop the remote from being built — it only costs
/// the append-merge special case for daily notes, which degrades to an ordinary
/// conflict (a side file), never to a lost line.
fn daily_folder(root: &Path) -> Option<String> {
    match VaultConfig::load(root) {
        Ok(Some(cfg)) => Some(cfg.daily.folder).filter(|f| !f.trim().is_empty()),
        Ok(None) => None,
        Err(e) => {
            eprintln!("garrulus: could not read the vault settings for the daily folder: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_config_round_trips_in_camel_case() {
        let cfg = RemoteConfig::git("upstream").with_branch(Some("notes".to_string()));
        let json = serde_json::to_string(&cfg).expect("serialises");

        assert!(json.contains("\"gitRemote\":\"upstream\""), "camelCase on the wire: {json}");
        assert!(json.contains("\"kind\":\"git\""), "kebab-case kind: {json}");

        let back: RemoteConfig = serde_json::from_str(&json).expect("parses back");
        assert_eq!(back, cfg);
    }

    #[test]
    fn folder_config_round_trips() {
        // Windows has no absolute path without a drive prefix, so `/mnt/...` would
        // fail `mirror_path`'s absoluteness check there rather than the serde
        // round trip this test is about.
        let raw = if cfg!(windows) { r"C:\mirror\vault" } else { "/mnt/usb/vault" };
        let cfg = RemoteConfig::folder(raw);
        let back: RemoteConfig =
            serde_json::from_str(&serde_json::to_string(&cfg).expect("serialises"))
                .expect("parses back");
        assert_eq!(back, cfg);
        assert_eq!(back.mirror_path().expect("valid"), PathBuf::from(raw));
    }

    /// The registry file is hand-editable and older entries will not carry the
    /// optional keys — every one of them has to default rather than fail.
    #[test]
    fn a_bare_kind_is_a_complete_git_config() {
        let cfg: RemoteConfig = serde_json::from_str(r#"{"kind":"git"}"#).expect("parses");
        assert_eq!(cfg.remote_name(), DEFAULT_GIT_REMOTE, "missing gitRemote means origin");
        assert_eq!(cfg.branch_name(), None);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn blank_strings_count_as_absent() {
        let cfg: RemoteConfig =
            serde_json::from_str(r#"{"kind":"git","gitRemote":"  ","branch":""}"#)
                .expect("parses");
        assert_eq!(cfg.remote_name(), DEFAULT_GIT_REMOTE, "an empty field is not a remote name");
        assert_eq!(cfg.branch_name(), None);
    }

    #[test]
    fn named_git_remote_and_branch_are_trimmed_not_defaulted() {
        let cfg: RemoteConfig =
            serde_json::from_str(r#"{"kind":"git","gitRemote":" backup ","branch":" main "}"#)
                .expect("parses");
        assert_eq!(cfg.remote_name(), "backup");
        assert_eq!(cfg.branch_name().as_deref(), Some("main"));
    }

    /// The rule that makes a folder destination trustworthy: no mirror path is an
    /// error the user sees, never a vault that quietly stays local-only.
    #[test]
    fn a_folder_without_a_folder_is_an_error() {
        let cfg: RemoteConfig = serde_json::from_str(r#"{"kind":"folder"}"#).expect("parses");
        assert!(cfg.mirror_path().is_err());
        let message = cfg.validate().expect_err("must not validate");
        assert!(message.contains("folder"), "the message names what is missing: {message}");

        let blank = RemoteConfig::folder("   ");
        assert!(blank.validate().is_err(), "whitespace is not a path");
    }

    /// A relative path would resolve against garrulus-be's working directory —
    /// never what the user picked.
    #[test]
    fn a_relative_mirror_path_is_refused() {
        let cfg = RemoteConfig::folder("mirror/vault");
        let message = cfg.validate().expect_err("must not validate");
        assert!(message.contains("absolute"), "the message says why: {message}");
    }

    #[test]
    fn mirror_path_is_only_asked_of_folder_destinations() {
        assert!(RemoteConfig::git(DEFAULT_GIT_REMOTE).mirror_path().is_err());
    }
}
