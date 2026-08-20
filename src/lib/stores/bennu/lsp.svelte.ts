/**
 * Language-server state: which servers exist, what they serve, and what they are doing.
 *
 * Two jobs, and the second is why this is a store rather than a set of calls:
 *
 * 1. **Routing.** Whether a file has a language server behind it decides which editor actions
 *    are offered at all (go-to, find-usages, rename, diagnostics). That question cannot be
 *    answered from a hard-coded extension list, because the user's own `[[lsp.servers]]` config
 *    adds languages the frontend has never heard of — so the extension set is *fetched* from the
 *    backend catalogue and cached here.
 * 2. **Status.** rust-analyzer takes tens of seconds to index a cold project and answers almost
 *    nothing until it has. A UI that hides that is a UI that looks broken, so the status line
 *    and its progress are pushed here as they change and read by the status bar.
 *
 * The events are **pushed** by the backend, not polled: a server's diagnostics arrive when
 * `cargo check` finishes, seconds after a save, and its progress changes several times a second
 * while it loads.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  lspServers, lspStatus, lspRestart, lspStop, lspInstall,
  type LspServerInfo, type LspStatus,
} from '$lib/ipc/bennu/lsp';
import type { SourceEdit } from '$lib/types/bennu';

/** Payload of `arbor://bennu/lsp-diagnostics`. */
interface DiagnosticsEvent {
  file: string;
  language: string;
  root: string;
}

/** Payload of `arbor://bennu/lsp-apply-edit` — a server-initiated edit. */
interface ApplyEditEvent {
  edits: SourceEdit[];
  /** Human descriptions of file operations Bennu will NOT perform. */
  file_ops: string[];
}

/** Payload of `arbor://bennu/lsp-message`. */
interface MessageEvent {
  level: 'error' | 'warning' | 'info' | 'log';
  message: string;
}

/** Lower-cased extension of `path`, without the dot. */
function extensionOf(path: string | null | undefined): string {
  if (!path) return '';
  const name = path.split(/[\\/]/).pop() ?? path;
  const dot = name.lastIndexOf('.');
  return dot >= 0 ? name.slice(dot + 1).toLowerCase() : '';
}

function createLspStore() {
  let servers = $state<LspServerInfo[]>([]);
  let installing = $state<string | null>(null);
  let statuses = $state<LspStatus[]>([]);
  /** The extensions of every ENABLED server, so routing is one lookup. */
  let servedExtensions = $state<Set<string>>(new Set());
  /** Semantic tokens currently painted — a diagnostic, see `tokenCount`. */
  let tokenCount = $state(0);

  /** Handlers the editor host registers for server-initiated traffic. */
  let onApplyEdit: ((edits: SourceEdit[], fileOps: string[]) => void) | null = null;
  let onDiagnostics: ((file: string) => void) | null = null;
  let onMessage: ((level: string, text: string) => void) | null = null;

  let attached = false;
  const unlisteners: UnlistenFn[] = [];

  function recomputeExtensions() {
    const set = new Set<string>();
    for (const s of servers) {
      if (!s.enabled) continue;
      for (const e of s.extensions) set.add(e.toLowerCase());
    }
    servedExtensions = set;
  }

  return {
    get servers() { return servers; },
    /** The id currently being installed, so its row can say so and the button can wait. */
    get installing() { return installing; },
    get statuses() { return statuses; },

    /** The servers that are installed and enabled — what actually has a chance of running. */
    get available() { return servers.filter((s) => s.enabled && !!s.path); },

    /** Servers that are enabled but whose binary was not found. Surfaced rather than hidden:
     *  each carries an install hint, and "why is there no Rust intelligence" deserves an answer. */
    get missing() { return servers.filter((s) => s.enabled && !s.path); },

    /** Whether a language server is (or would be) the engine for `path`.
     *
     *  Note what this does NOT check: whether the server is up. It must not — a `.rs` file is
     *  Rust's whether rust-analyzer has finished starting or crashed, and treating it as
     *  otherwise would route it to the Java engine, which answers about Java. */
    servesFile(path: string | null | undefined): boolean {
      const ext = extensionOf(path);
      return !!ext && servedExtensions.has(ext);
    },

    /** The status of the server serving `path`, when there is a slot for one. */
    statusFor(path: string | null | undefined): LspStatus | null {
      const ext = extensionOf(path);
      if (!ext) return null;
      const server = servers.find((s) => s.extensions.some((e) => e.toLowerCase() === ext));
      if (!server) return null;

      const sameLanguage = statuses.filter((s) => s.language === server.language);
      if (!sameLanguage.length) return null;

      // Several roots can be open with the same language; the one whose root is a prefix of the
      // file owns it, and the longest such root wins (a workspace member opened separately).
      const p = (path ?? '').replace(/\\/g, '/');
      let best: LspStatus | null = null;
      for (const s of sameLanguage) {
        const root = s.root.endsWith('/') ? s.root : `${s.root}/`;
        if (!p.startsWith(root)) continue;
        if (!best || s.root.length > best.root.length) best = s;
      }
      // No root matched, yet a server for this language is running. Rather than report "no
      // server" — which reads as broken and hides the status pill — fall back to the only one
      // there is.
      //
      // The prefix test is not as reliable as it looks: on macOS `/tmp` and `/var` are symlinks
      // into `/private`, so a canonicalised root and the path the editor holds can describe the
      // same file with different strings. With one session for the language there is nothing to
      // disambiguate, so the answer is unambiguous even when the strings disagree; with several,
      // guessing would be worse than admitting it.
      return best ?? (sameLanguage.length === 1 ? sameLanguage[0] : null);
    },

    /**
     * The status to show for the **open project**, whatever file happens to be in front of you.
     *
     * The per-file lookup is right for deciding what an editor gesture does and wrong for a status
     * strip: a server running for this project is a fact about the project, and hiding it while you
     * look at `Cargo.toml` or a README removes the answer to "is it still indexing" exactly when
     * that is the question. Prefers the file's own server when there is one.
     *
     * The longest root containing `root` wins, so a workspace member opened in its own right
     * reports its own server.
     */
    statusForProject(
      root: string | null | undefined,
      activeFile?: string | null,
    ): LspStatus | null {
      const own = this.statusFor(activeFile);
      if (own) return own;
      if (!root) return null;
      const r = root.replace(/\\/g, '/').replace(/\/$/, '');
      let best: LspStatus | null = null;
      for (const s of statuses) {
        const sr = s.root.replace(/\/$/, '');
        // Either direction: the session may cover a Cargo workspace above the opened folder, or
        // sit inside it (a member started for a file the user opened from elsewhere).
        if (!(sr === r || r.startsWith(`${sr}/`) || sr.startsWith(`${r}/`))) continue;
        if (!best || sr.length > best.root.length) best = s;
      }
      return best;
    },

    /** Whether a server can serve `feature` for `path` right now. */
    supports(path: string | null | undefined, feature: string): boolean {
      const status = this.statusFor(path);
      return !!status && status.state === 'ready' && status.features.includes(feature);
    },

    /**
     * How many semantic tokens are painted in the editor right now.
     *
     * A diagnostic, and it earns its place: "the file is all white" has two completely different
     * causes — the tokens never arrived, or they arrived and lost the colour fight — and they are
     * indistinguishable on screen. This number separates them, from a tooltip, without a debugger.
     */
    get tokenCount() { return tokenCount; },
    setTokenCount(n: number) { tokenCount = n; },

    /** Register the editor host's handlers for server-initiated traffic. Returns a detach fn. */
    onServerEdit(handler: (edits: SourceEdit[], fileOps: string[]) => void): () => void {
      onApplyEdit = handler;
      return () => { if (onApplyEdit === handler) onApplyEdit = null; };
    },

    /** Register a callback for "the server published diagnostics for this file". */
    onDiagnosticsPublished(handler: (file: string) => void): () => void {
      onDiagnostics = handler;
      return () => { if (onDiagnostics === handler) onDiagnostics = null; };
    },

    /** Register a callback for a server's `window/showMessage`. */
    onServerMessage(handler: (level: string, text: string) => void): () => void {
      onMessage = handler;
      return () => { if (onMessage === handler) onMessage = null; };
    },

    /** Re-read the catalogue (after a settings change). */
    async reloadServers(): Promise<void> {
      servers = await lspServers().catch(() => []);
      recomputeExtensions();
    },

    /** Re-read the live statuses. */
    async reloadStatuses(): Promise<void> {
      statuses = await lspStatus().catch(() => []);
    },

    /**
     * Install a server by running the command its ecosystem ships it through.
     *
     * Answers with the outcome instead of throwing, because both endings are ordinary here:
     * a `cargo install --git` builds from source and can fail on a toolchain that is too
     * old, and the caller has to say something either way. The catalogue is re-read on the
     * way out, so a success flips the row from "not found" to a path without a reload.
     */
    async install(id: string): Promise<{ ok: boolean; message: string }> {
      installing = id;
      try {
        const res = await lspInstall(id);
        await this.reloadServers();
        if (res.ok) return { ok: true, message: `Installed — ${res.path}` };
        // The diagnosis when there is one, the output when there is not, and a pointer at
        // the panel when there was nothing to say at all.
        return {
          ok: false,
          message: res.hint || res.tail.trim() || `\`${res.command}\` failed. The Build panel has the log.`,
        };
      } catch (e) {
        return { ok: false, message: e instanceof Error ? e.message : String(e) };
      } finally {
        installing = null;
      }
    },

    /** Restart a server — the way out of a failed slot, and the fix for "I just installed it". */
    async restart(root: string, language: string): Promise<void> {
      await lspRestart(root, language).catch(() => false);
      await this.reloadStatuses();
    },

    /** Stop a server. */
    async stop(root: string, language: string): Promise<void> {
      await lspStop(root, language).catch(() => false);
      await this.reloadStatuses();
    },

    /**
     * Subscribe to the backend's pushes and load the initial state. Returns a detach fn.
     *
     * The `lspStatus()` call is not just a read: it is what hands the backend its event sink, so
     * without it nothing would ever be pushed.
     */
    async attach(): Promise<() => void> {
      if (attached) return () => {};
      attached = true;

      unlisteners.push(
        await listen<LspStatus[]>('arbor://bennu/lsp-status', (e) => {
          statuses = e.payload ?? [];
        }),
      );
      unlisteners.push(
        await listen<DiagnosticsEvent>('arbor://bennu/lsp-diagnostics', (e) => {
          const file = e.payload?.file;
          if (file) onDiagnostics?.(file);
        }),
      );
      unlisteners.push(
        await listen<ApplyEditEvent>('arbor://bennu/lsp-apply-edit', (e) => {
          const edits = e.payload?.edits ?? [];
          const ops = e.payload?.file_ops ?? [];
          if (edits.length || ops.length) onApplyEdit?.(edits, ops);
        }),
      );
      unlisteners.push(
        await listen<MessageEvent>('arbor://bennu/lsp-message', (e) => {
          const p = e.payload;
          // A server's log stream is verbose by design and already goes to stderr; only what it
          // chose to *show* reaches the user.
          if (p && p.level !== 'log') onMessage?.(p.level, p.message);
        }),
      );

      await Promise.all([this.reloadServers(), this.reloadStatuses()]);

      return () => {
        for (const u of unlisteners.splice(0)) u();
        attached = false;
      };
    },
  };
}

export const bennuLspStore = createLspStore();
