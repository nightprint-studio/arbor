//! [`GarrulusState`] — the headless garrulus backend's owned state.
//!
//! Modelled on `corvus-core`'s `CorvusState` (event egress + reverse channel +
//! hook broker) plus the three pieces a note vault genuinely keeps for the life of
//! a session: the open [`Vault`], the [`Index`] over it, and the configured
//! [`SyncRemote`].
//!
//! ## Locking discipline (read this before writing a handler)
//!
//! Every accessor hands back a guard with the poison already mapped to a wire
//! string, so a handler reads:
//!
//! ```ignore
//! let root = state.vault_root()?;              // guard taken and dropped inside
//! let note = load_note(&root, &path)?;         // no lock held across the I/O
//! state.index_write()?.upsert(note);           // guard dropped at the semicolon
//! state.fire_hook(hooks::NOTE_SAVED, json!({ … }));     // fired with NO lock held
//! ```
//!
//! Three rules make that shape mandatory rather than stylistic:
//!
//! 1. **Fire hooks after dropping every guard.** A hook runs Lua synchronously and
//!    that Lua may call back into a garrulus RPC method; holding the index or
//!    vault lock across the fire deadlocks the process. Same rule corvus-be states
//!    for its repo handles.
//! 2. **Never hold a guard across an `.await`, a blocking network call, or a
//!    `host_call`.** The remote is the one that used to break this: a pull, a push
//!    and even a probe reach the shell's credential broker over the reverse
//!    channel, so a reader parked there is parked for a whole off-machine round
//!    trip. It bought nothing — an `RwLock` read guard is *shared*, so holding it
//!    never serialised two concurrent syncs — and it cost the write side: a
//!    `garrulus_set_remote` arriving mid-probe waits out the whole round trip, and
//!    `std::sync::RwLock` makes no promise that later readers overtake a waiting
//!    writer (the platform decides), so one slow probe can stall the sync button
//!    behind it. Hence [`remote`](GarrulusState::remote), which hands back an `Arc`
//!    and drops the guard before the caller drives anything.
//! 3. **Lock order is vault → index**, and the shape above avoids the question
//!    entirely by never nesting the two: take what the vault knows, drop that
//!    guard, then take the index. The remote is no longer in that order at all —
//!    its guard never leaves this module, so it cannot be nested with anything.
//!    Keep it that way — a nested take in the other order is the one bug this
//!    module cannot detect for you.

use std::path::PathBuf;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use arbor_ipc::prelude::{EventSink, HostCaller};
use arbor_plugin_api::prelude::{HookDispatcher, PluginValue};
use garrulus_index::prelude::Index;
use garrulus_sync::prelude::SyncRemote;
use garrulus_vault::prelude::{Note, Vault};
use serde_json::Value;

/// The state every garrulus-be handler gets, `Arc`-shared across the dispatcher
/// and the background workers (the vault watcher, the sync probe).
pub struct GarrulusState {
    /// Backend → frontend event egress. The shell re-emits each topic to the
    /// Garrulus window. Call sites use [`emit`](Self::emit) /
    /// [`event_sink`](Self::event_sink).
    events: Arc<dyn EventSink>,
    /// Reverse channel back to the shell (`docs/reverse-channel.md`), set from the
    /// `App`'s host caller — this is how the sync engine asks the shell's
    /// credential broker for a token. `None` only in the (unused) in-process path.
    host: Option<Arc<dyn HostCaller>>,
    /// Runtime hook broker, so a handler fires its vault hooks where it runs. The
    /// default is an empty dispatcher (no listener) → fires are clean no-ops,
    /// which is what garrulus-be gets until it wires a plugin host.
    hooks: Arc<HookDispatcher>,
    /// The open vault — one per process, `None` before the first
    /// `garrulus_open_vault` and after `garrulus_close_vault`.
    vault: RwLock<Option<Vault>>,
    /// The link graph + search index over [`vault`](Self::vault). A **cache**:
    /// rebuilt wholesale at vault open, upserted per note on save, and safe to
    /// throw away — never the record.
    index: RwLock<Index>,
    /// The configured sync destination, or `None` for a vault with no remote (the
    /// `no-remote` state of the sync button). A trait object because the
    /// implementation is chosen at runtime (`GitRemote` / `FolderRemote`), and an
    /// `Arc` rather than a `Box` so [`remote`](Self::remote) can clone the handle
    /// out and let the guard go before the caller drives a network round trip.
    remote: RwLock<Option<Arc<dyn SyncRemote>>>,
}

impl GarrulusState {
    /// Build the backend state from its event egress. Wave-friendly: a new piece
    /// gains a `with_*` builder rather than a new constructor.
    pub fn new(events: Arc<dyn EventSink>) -> Self {
        Self {
            events,
            host: None,
            hooks: Arc::new(HookDispatcher::new()),
            vault: RwLock::new(None),
            index: RwLock::new(Index::build(Vec::new())),
            remote: RwLock::new(None),
        }
    }

    /// Attach the reverse channel back to the shell (the `App`'s host caller).
    pub fn with_host_caller(mut self, host: Arc<dyn HostCaller>) -> Self {
        self.host = Some(host);
        self
    }

    /// Attach the hook broker. `garrulus-be` passes the dispatcher its plugin host
    /// builds; without one the default empty dispatcher makes every fire a no-op.
    pub fn with_hooks(mut self, hooks: Arc<HookDispatcher>) -> Self {
        self.hooks = hooks;
        self
    }

    // ── Egress / reverse channel ──────────────────────────────────────────────

    /// Emit a frontend event. The shell re-emits the topic to the Garrulus window.
    pub fn emit(&self, topic: &str, payload: Value) {
        self.events.emit(topic, payload);
    }

    /// A cloneable handle to the event egress, for a background worker (the vault
    /// watcher) that emits from inside and outlives the borrow of `&self`.
    pub fn event_sink(&self) -> Arc<dyn EventSink> {
        Arc::clone(&self.events)
    }

    /// Call back into the shell, blocking on the reply. Errors with a clear
    /// message when no reverse channel is wired.
    pub fn host_call(&self, method: &str, params: Value) -> Result<Value, String> {
        match &self.host {
            Some(h) => h.call(method, params),
            None => Err(format!("host_call('{method}'): no reverse channel (in-process)")),
        }
    }

    /// A cloneable handle to the reverse channel, for a background worker (the
    /// sync probe, whose credential callback must reach the shell's broker).
    pub fn host_caller(&self) -> Option<Arc<dyn HostCaller>> {
        self.host.clone()
    }

    /// Fire a fire-and-forget vault hook to every subscriber, synchronously.
    /// **Call this with no lock held** — see the module's locking discipline.
    ///
    /// `hook` is a constant from [`crate::hooks`], never a literal: an unknown
    /// name is not an error here, it is silence — the dispatcher looks it up,
    /// misses, and returns.
    pub fn fire_hook(&self, hook: &str, ctx: Value) {
        self.hooks.fire_blocking(hook, PluginValue::from_json(ctx));
    }

    /// A cloneable handle to the hook broker, for a background worker that fires
    /// past the borrow of `&self`.
    pub fn hooks_handle(&self) -> Arc<HookDispatcher> {
        Arc::clone(&self.hooks)
    }

    // ── Vault ─────────────────────────────────────────────────────────────────

    /// Read the open vault. `Err` only on a poisoned lock (a handler panicked
    /// mid-write); the "no vault open" case is the `None` inside.
    pub fn vault_read(&self) -> Result<RwLockReadGuard<'_, Option<Vault>>, String> {
        self.vault.read().map_err(|_| poisoned("vault"))
    }

    /// Take the write side of the open vault (open / close / reload its types).
    pub fn vault_write(&self) -> Result<RwLockWriteGuard<'_, Option<Vault>>, String> {
        self.vault.write().map_err(|_| poisoned("vault"))
    }

    /// The open vault's root directory, or the canonical "nothing open" error.
    /// Takes and drops the read guard, so the caller can do I/O lock-free — which
    /// is why nearly every handler starts here.
    pub fn vault_root(&self) -> Result<PathBuf, String> {
        match self.vault_read()?.as_ref() {
            Some(v) => Ok(v.root.clone()),
            None => Err("no vault is open".to_string()),
        }
    }

    /// Install a freshly opened vault, replacing any previous one.
    pub fn set_vault(&self, vault: Vault) -> Result<(), String> {
        *self.vault_write()? = Some(vault);
        Ok(())
    }

    /// Close the open vault and empty the index. Returns the root that was open,
    /// so the caller can stop the watcher and name it in the hook payload.
    pub fn close_vault(&self) -> Result<Option<PathBuf>, String> {
        let previous = self.vault_write()?.take().map(|v| v.root);
        self.rebuild_index(Vec::new())?;
        self.clear_remote()?;
        Ok(previous)
    }

    // ── Index ─────────────────────────────────────────────────────────────────

    /// Read the index (search, backlinks, quick switch, problems).
    pub fn index_read(&self) -> Result<RwLockReadGuard<'_, Index>, String> {
        self.index.read().map_err(|_| poisoned("index"))
    }

    /// Take the write side of the index (upsert on save, remove on delete).
    pub fn index_write(&self) -> Result<RwLockWriteGuard<'_, Index>, String> {
        self.index.write().map_err(|_| poisoned("index"))
    }

    /// Replace the index wholesale — the vault-open path and the "rebuild index"
    /// command. Cheap enough at personal-vault scale (thousands of notes) that
    /// incremental repair is not worth its bug surface.
    pub fn rebuild_index(&self, notes: Vec<Note>) -> Result<(), String> {
        *self.index_write()? = Index::build(notes);
        Ok(())
    }

    // ── Sync remote ───────────────────────────────────────────────────────────

    /// The configured sync destination, as an **owned handle**.
    ///
    /// The guard is taken and dropped inside, which is the entire point: every
    /// `SyncRemote` method reaches the network and can reach the shell's credential
    /// broker over the reverse channel, and a call site that drove one while
    /// holding a guard would park a reader for a whole off-machine round trip —
    /// see rule 2 of the module's locking discipline for why that is expensive even
    /// though it is not a deadlock.
    ///
    /// `Ok(None)` is the ordinary "this vault syncs nowhere" answer (the sync
    /// button's `no-remote` state), not a failure; `Err` is only a poisoned lock.
    ///
    /// There is deliberately **no guard-returning accessor** next to this one: the
    /// slot's write side is reached through [`set_remote`](Self::set_remote) /
    /// [`clear_remote`](Self::clear_remote), so the shape that caused the stall
    /// cannot be written again from outside this module.
    pub fn remote(&self) -> Result<Option<Arc<dyn SyncRemote>>, String> {
        Ok(self.remote.read().map_err(|_| poisoned("remote"))?.clone())
    }

    /// The write side of the remote slot. Private, and the only thing that takes
    /// it — see [`remote`](Self::remote).
    fn remote_slot(&self) -> Result<RwLockWriteGuard<'_, Option<Arc<dyn SyncRemote>>>, String> {
        self.remote.write().map_err(|_| poisoned("remote"))
    }

    /// Install (or replace) the vault's sync destination.
    ///
    /// Takes the `Box` the factory hands back and promotes it to the `Arc` the slot
    /// holds, so `crate::remote::build_remote` stays the one place a remote is
    /// constructed and no call site has to know how it is stored.
    pub fn set_remote(&self, remote: Box<dyn SyncRemote>) -> Result<(), String> {
        let shared: Arc<dyn SyncRemote> = Arc::from(remote);
        *self.remote_slot()? = Some(shared);
        Ok(())
    }

    /// Detach the sync destination, leaving the vault local-only.
    ///
    /// Load-bearing on every vault switch: a `GitRemote` captures the vault path
    /// it was built for and ignores the one it is later handed, so a remote left
    /// over from the previous vault would happily commit and push into *that*
    /// vault while the user is looking at this one.
    pub fn clear_remote(&self) -> Result<(), String> {
        *self.remote_slot()? = None;
        Ok(())
    }
}

/// One phrasing for every poisoned-lock error, so the wire string is predictable
/// whichever accessor produced it (the error strings ARE the seam's contract).
fn poisoned(what: &str) -> String {
    format!("garrulus: the {what} lock is poisoned (a handler panicked); reopen the vault")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poisoned_message_names_the_lock() {
        assert!(poisoned("index").contains("index"));
        assert!(poisoned("vault").contains("vault"));
    }
}
