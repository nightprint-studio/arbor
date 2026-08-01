//! Bringing remote changes in — the half of [`GitRemote`] where the product
//! either earns the user's trust or loses it.
//!
//! Three shapes, in order of how often they happen:
//!
//! 1. **Fast-forward** — the other machine moved and this one did not. `git
//!    merge --ff-only` and nothing else needs saying.
//! 2. **Purely ahead** — nothing to do; that is a push, not a pull.
//! 3. **Diverged** — both moved. Merge with `--no-commit`, then resolve every
//!    conflicted file *ourselves*, one by one: `.arbor/garrulus/` metadata is
//!    merged by rule and never reported, daily notes append-merge, other notes
//!    merge frontmatter field-wise and body three-way, and whatever is left
//!    keeps the local text with the remote parked in a side file beside it.
//!
//! The invariant that makes case 3 acceptable
//! (`docs/garrulus-design.md` §4.4): a note's bytes are only ever written by
//! [`GitRemote::take_text`], which writes a single decided version. Git's own
//! conflicted working-tree file — the one with the `<<<<<<<` in it — is
//! overwritten before the merge is committed, so no marker ever survives into
//! the vault and the vault still opens in Obsidian mid-conflict.

use corvus_git::prelude::GitRepo;

use crate::change::RelPath;
use crate::conflict::{
    append_merge_daily, is_daily_note, is_side_file, merge_note, side_file_name, Conflict,
    ConflictStamp,
};
use crate::error::{SyncError, SyncResult};
use crate::files::write_note;
use crate::metadata::{is_metadata_path, merge_metadata};
use crate::remote::PullOutcome;

use super::GitRemote;

impl GitRemote {
    /// Fetch, then apply whatever came in. See the module docs for the shapes.
    pub(super) fn pull_blocking(&self) -> SyncResult<PullOutcome> {
        let repo = self.open()?;
        if !self.has_remote(&repo) {
            return Err(SyncError::NotConfigured(format!("no remote named '{}'", self.remote)));
        }
        let branch = self.branch_name(&repo)?;
        self.fetch(&repo)?;

        let upstream = self.upstream_ref(&branch);
        let (Some(local), Some(remote)) = (self.rev("HEAD"), self.rev(&upstream)) else {
            return Ok(PullOutcome::default());
        };
        if local == remote {
            return Ok(PullOutcome::default());
        }
        let base = self
            .merge_base(&local, &remote)
            .ok_or_else(|| SyncError::Git("no common history with the remote".into()))?;
        if base == remote {
            return Ok(PullOutcome::default()); // purely ahead
        }
        if base == local {
            self.run(&["merge", "--ff-only", remote.as_str()])?;
            return Ok(PullOutcome {
                applied: self.changed_paths(&local, &remote)?,
                conflicts: Vec::new(),
            });
        }
        self.merge_diverged(&repo, &local, &remote, &base)
    }

    /// Both sides moved. Merge, then resolve every conflicted note in the
    /// working tree ourselves before completing the commit.
    fn merge_diverged(
        &self,
        repo: &GitRepo,
        local: &str,
        remote: &str,
        base: &str,
    ) -> SyncResult<PullOutcome> {
        let attempt = self.try_run(&["merge", "--no-commit", "--no-ff", remote])?;
        let conflicted = if attempt.ok { Vec::new() } else { self.conflicted_paths()? };
        if !attempt.ok && conflicted.is_empty() && !self.in_merge() {
            // Not a conflict: a real failure (dirty tree, hook refusal, …).
            return Err(SyncError::from_git_message(attempt.stderr));
        }

        let stamp = ConflictStamp::now();
        let mut conflicts: Vec<Conflict> = Vec::new();
        for rel in &conflicted {
            let base_text = self.file_at(repo, base, rel);
            let local_text = self.file_at(repo, local, rel);
            let remote_text = self.file_at(repo, remote, rel);
            // The vault's own metadata is settled by rule before anything else
            // looks at it: it is not a note, and it never conflicts (§4.4.4).
            if is_metadata_path(rel) {
                self.resolve_metadata(
                    rel,
                    base_text,
                    local_text,
                    remote_text,
                    stamp,
                    &mut conflicts,
                )?;
                continue;
            }
            match (local_text, remote_text) {
                (Some(l), Some(r)) => {
                    self.resolve_both_sides(rel, base_text, l, r, stamp, &mut conflicts)?;
                }
                // An edit always outranks a delete: the note comes back rather
                // than a paragraph disappearing because the other machine tidied.
                (Some(l), None) => self.take_text(rel, &l)?,
                (None, Some(r)) => self.take_text(rel, &r)?,
                (None, None) => {
                    self.try_run(&["rm", "-f", "--ignore-unmatch", "--", rel.as_str()])?;
                }
            }
        }

        if self.in_merge() {
            let msg = format!("Unione da {} ({})", self.remote, self.device);
            self.commit_as_device(&msg)?;
        }

        let head = self.rev("HEAD").unwrap_or_else(|| local.to_string());
        let mut applied = self.changed_paths(local, &head)?;
        applied.retain(|p| !conflicts.iter().any(|c| &c.path == p) && !is_side_file(p));
        Ok(PullOutcome { applied, conflicts })
    }

    /// One note both machines edited.
    fn resolve_both_sides(
        &self,
        rel: &RelPath,
        base: Option<String>,
        local: String,
        remote: String,
        stamp: ConflictStamp,
        conflicts: &mut Vec<Conflict>,
    ) -> SyncResult<()> {
        if is_daily_note(rel, self.daily_folder.as_deref()) {
            let merged = append_merge_daily(base.as_deref(), &local, &remote);
            return self.take_text(rel, &merged);
        }
        if let Some(merged) = merge_note(base.as_deref(), &local, &remote) {
            return self.take_text(rel, &merged);
        }
        // Nothing automatic left: keep local, park remote beside it.
        self.park_remote(rel, base, local, remote, stamp, conflicts)
    }

    /// One `.arbor/garrulus/` file both machines edited.
    ///
    /// Merged by rule — union of the type set, per-key last-writer-wins on
    /// settings — because a conflict in a settings file is pure noise: the user
    /// never opened `vault.toml`, they ticked a box, and a side file called
    /// `vault (conflitto — casa, 31-07 14:22).toml` is a question they cannot
    /// answer (§4.4.4).
    ///
    /// The one thing that still arbitrates is a file the merger cannot read at
    /// all: refusing to touch it costs a visible side file, whereas guessing at
    /// its shape would silently drop a setting the user changed.
    fn resolve_metadata(
        &self,
        rel: &RelPath,
        base: Option<String>,
        local: Option<String>,
        remote: Option<String>,
        stamp: ConflictStamp,
        conflicts: &mut Vec<Conflict>,
    ) -> SyncResult<()> {
        // Bound before the match, not matched on directly: the borrows the call
        // takes of `local` and `remote` have to end before an arm can move them.
        let merged = merge_metadata(base.as_deref(), local.as_deref(), remote.as_deref());
        match merged {
            Some(Some(text)) => self.take_text(rel, &text),
            Some(None) => {
                self.try_run(&["rm", "-f", "--ignore-unmatch", "--", rel.as_str()])?;
                Ok(())
            }
            // Unreadable on one side, and only ever with both sides present:
            // every one-sided case has already been decided by the merger.
            None => match (local, remote) {
                (Some(l), Some(r)) => self.park_remote(rel, base, l, r, stamp, conflicts),
                (Some(l), None) => self.take_text(rel, &l),
                (None, Some(r)) => self.take_text(rel, &r),
                (None, None) => Ok(()),
            },
        }
    }

    /// Keep the local text, park the remote one beside it, and report the pair
    /// so the conflicts dock can offer *keep mine* / *take theirs* / *merge by
    /// hand*. The only place a side file is ever created.
    fn park_remote(
        &self,
        rel: &RelPath,
        base: Option<String>,
        local: String,
        remote: String,
        stamp: ConflictStamp,
        conflicts: &mut Vec<Conflict>,
    ) -> SyncResult<()> {
        self.take_text(rel, &local)?;
        let side = side_file_name(rel, &self.device, stamp);
        write_note(&side.to_path(&self.vault), &remote)?;
        self.run(&["add", "--", side.as_str()])?;
        conflicts.push(Conflict {
            path: rel.clone(),
            base,
            local,
            remote,
            side_file: Some(side),
        });
        Ok(())
    }

    /// Put `text` in the working tree and stage it — the only way a note's
    /// bytes are ever decided, so no merge marker can leak in.
    fn take_text(&self, rel: &RelPath, text: &str) -> SyncResult<()> {
        write_note(&rel.to_path(&self.vault), text)?;
        self.run(&["add", "--", rel.as_str()])?;
        Ok(())
    }
}
