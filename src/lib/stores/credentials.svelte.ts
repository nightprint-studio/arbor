/**
 * Cross-product access to the credentials dialog.
 *
 * The credential broker itself is process-wide (the shell's keychain vault), so
 * resolving a token never needs a window. Re-authenticating does: connecting a
 * provider, replacing a revoked token, adding a host that has no OAuth
 * connector. That UI used to live only inside Corvus's Settings, which left the
 * File Explorer — which runs git operations of its own — with no way out of a
 * 401 unless the Git window happened to be open.
 *
 * The dialog is now mounted in every window (see `+page.svelte`) and raised
 * through this store, so any surface can offer the way back: a menu entry, a
 * command palette verb, or an auth failure handler.
 */
function createCredentialsStore() {
  let open = $state(false);

  return {
    get open() { return open; },
    /** Raise the credentials dialog in the current window. */
    show()  { open = true; },
    close() { open = false; },
  };
}

export const credentialsStore = createCredentialsStore();
