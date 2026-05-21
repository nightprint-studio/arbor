<script lang="ts">
  /**
   * Pre-install permission confirmation modal.
   *
   * Surfaces the resolved `[permissions]` block from the plugin's
   * `plugin.toml` as a human-readable list with severity colouring, so the
   * user makes an informed choice before the host downloads the zipball.
   *
   * Wired by `MarketplaceModal` — the modal opens this confirm before
   * calling `marketplace_install_plugin`; cancelling here is a no-op.
   */
  import { Shield, Globe, FolderGit2, Terminal, Variable, KeyRound, Wrench, MessagesSquare, ArrowDownToLine, Network, Download, AlertTriangle } from 'lucide-svelte';
  import Modal       from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button      from '$lib/components/shared/ui/Button.svelte';
  import Alert       from '$lib/components/shared/ui/Alert.svelte';
  import type { MarketplacePlugin } from '$lib/types/marketplace';

  let {
    plugin,
    onCancel,
    onConfirm,
  }: {
    plugin:    MarketplacePlugin;
    onCancel:  () => void;
    onConfirm: () => void;
  } = $props();

  /** A single row in the permissions list. */
  type Row = {
    icon: typeof Globe;
    title: string;
    detail: string;
    tone: 'safe' | 'warn' | 'danger';
  };

  function buildRows(p: MarketplacePlugin): Row[] {
    const perms = p.permissions;
    if (!perms) return [];
    const rows: Row[] = [];

    // Network
    if (perms.network && perms.network.length > 0) {
      rows.push({
        icon: Globe,
        title: 'Network access',
        detail: `Can contact: ${perms.network.join(', ')}`,
        tone: perms.network.includes('*') ? 'danger' : 'warn',
      });
    }

    // Filesystem
    if (perms.fs !== 'none') {
      const wildcard = perms.fs_scope?.includes('*');
      const scope = wildcard
        ? 'any path on disk'
        : perms.fs_scope && perms.fs_scope.length > 0
          ? `${perms.fs_scope.length} listed paths + active repo`
          : 'active repo only';
      rows.push({
        icon: FolderGit2,
        title: `Filesystem (${perms.fs})`,
        detail: `Scope: ${scope}`,
        tone: wildcard ? 'danger' : perms.fs === 'write' ? 'warn' : 'safe',
      });
    }

    // Git
    if (perms.git !== 'none') {
      const detail = perms.git === 'history_rewrite'
        ? 'rebase / reset --hard / force-push / amend (destructive)'
        : perms.git === 'write'
          ? 'commit / branch / fetch / push / clone / stash'
          : 'read-only graph + status access';
      rows.push({
        icon: ArrowDownToLine,
        title: `Git (${perms.git})`,
        detail,
        tone: perms.git === 'history_rewrite' ? 'danger' : perms.git === 'write' ? 'warn' : 'safe',
      });
    }

    // Terminal
    if (perms.terminal !== 'none') {
      const detail = perms.terminal === 'any'
        ? 'arbitrary shell commands (full terminal access)'
        : `command allowlist: ${(perms.terminal_scope ?? []).join(', ') || '(empty)'}`;
      rows.push({
        icon: Terminal,
        title: `Terminal (${perms.terminal})`,
        detail,
        tone: perms.terminal === 'any' ? 'danger' : 'warn',
      });
    }

    // Env read
    const envRead = perms.env_read;
    if (envRead !== false) {
      const detail = envRead === true
        ? 'all environment variables readable'
        : Array.isArray(envRead)
          ? `allowlist: ${envRead.join(', ')}`
          : '';
      if (detail) {
        rows.push({
          icon: Variable,
          title: 'Environment variables',
          detail,
          tone: envRead === true ? 'warn' : 'safe',
        });
      }
    }

    // Issues
    if (perms.issues !== 'none') {
      rows.push({
        icon: MessagesSquare,
        title: `Issue tracker (${perms.issues})`,
        detail: perms.issues === 'write'
          ? 'search, transition, comment on Linear / Jira issues'
          : 'search and read Linear / Jira issues',
        tone: perms.issues === 'write' ? 'warn' : 'safe',
      });
    }

    // Provider (GitHub / GitLab APIs)
    if (perms.provider && perms.provider !== 'none') {
      rows.push({
        icon: Network,
        title: `Git provider API (${perms.provider})`,
        detail: 'read MR / PR / CI data via the host\'s authenticated token',
        tone: perms.provider === 'write' ? 'warn' : 'safe',
      });
    }

    // Toolchain
    if (perms.toolchain && perms.toolchain !== 'none') {
      rows.push({
        icon: Wrench,
        title: `Toolchain (${perms.toolchain})`,
        detail: perms.toolchain === 'write'
          ? 'add / remove / set active JDK, Node, Rust toolchains'
          : 'read installed toolchains + active selection',
        tone: perms.toolchain === 'write' ? 'warn' : 'safe',
      });
    }

    // Service export / call (cross-plugin)
    if (perms.service_export || perms.service_call) {
      const bits: string[] = [];
      if (perms.service_export) bits.push('export services');
      if (perms.service_call)   bits.push('call other plugins');
      rows.push({
        icon: KeyRound,
        title: 'Cross-plugin services',
        detail: bits.join(' + '),
        tone: 'safe',
      });
    }

    if (perms.settings_read_others) {
      rows.push({
        icon: KeyRound,
        title: 'Read other plugins\' settings',
        detail: 'can read but not write other plugins\' global / project settings',
        tone: 'safe',
      });
    }

    return rows;
  }

  const rows = $derived(buildRows(plugin));
  const hasDanger = $derived(rows.some(r => r.tone === 'danger'));
</script>

<Modal onClose={onCancel} width="min(640px, 92vw)" padBody={false} ariaLabel="Confirm install">
  {#snippet header()}
    <ModalHeader onClose={onCancel}>
      <Shield size={14} />
      <span class="modal-title">Confirm install — {plugin.name}</span>
    </ModalHeader>
  {/snippet}

  <div class="mic-root">
    <header class="mic-header">
      <div class="mic-id">
        <span class="mic-name">{plugin.name}</span>
        <span class="mic-ver">v{plugin.version}</span>
      </div>
      <p class="mic-desc">{plugin.description}</p>
    </header>

    {#if hasDanger}
      <Alert variant="warning" title="High-impact permissions requested">
        This plugin asks for capabilities that can read or modify data outside its sandbox.
        Review carefully — install only if you trust the author.
      </Alert>
    {/if}

    <section class="mic-section">
      <h4><Shield size={11} /> The plugin will be able to:</h4>
      {#if rows.length === 0}
        <p class="mic-muted">No elevated permissions requested. The plugin runs in the default sandbox.</p>
      {:else}
        <ul class="mic-list">
          {#each rows as r, i (i)}
            <li class="mic-row mic-tone-{r.tone}">
              <r.icon size={14} class="mic-row-icon" />
              <div class="mic-row-body">
                <span class="mic-row-title">{r.title}</span>
                <span class="mic-row-detail">{r.detail}</span>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="mic-section mic-source">
      <span class="mic-muted small">
        Will download <code>{plugin.entry.repo}</code>
        {#if plugin.entry.subpath}/<code>{plugin.entry.subpath}</code>{/if}
        @ <code>{plugin.entry.ref ?? 'main'}</code>
      </span>
    </section>
  </div>

  {#snippet footer()}
    <ModalFooter>
      <Button variant="ghost" onclick={onCancel}>Cancel</Button>
      <Button variant="primary" onclick={onConfirm}>
        {#snippet iconStart()}<Download size={14} />{/snippet}
        Install
      </Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .mic-root {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
    background: var(--bg-base);
  }

  .mic-header {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .mic-id {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .mic-name {
    font-weight: 600;
    font-size: 14px;
  }
  .mic-ver {
    color: var(--text-secondary);
    font-family: var(--font-mono, monospace);
    font-size: 11px;
  }
  .mic-desc {
    margin: 0;
    color: var(--text-secondary);
    font-size: 12px;
    line-height: 1.45;
  }

  .mic-section {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    padding: 10px 12px;
  }
  .mic-section h4 {
    margin: 0 0 8px;
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-secondary);
  }

  .mic-muted {
    margin: 0;
    color: var(--text-secondary);
    font-size: 12px;
  }
  .mic-muted.small {
    font-size: 11px;
  }

  .mic-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .mic-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 4px;
    border-left: 2px solid transparent;
    background: var(--bg-base);
  }
  .mic-row :global(.mic-row-icon) {
    margin-top: 2px;
    flex: 0 0 auto;
  }
  .mic-row-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .mic-row-title {
    font-weight: 600;
    font-size: 12px;
  }
  .mic-row-detail {
    color: var(--text-secondary);
    font-size: 11px;
    word-break: break-word;
  }
  .mic-tone-safe   { border-left-color: var(--success); }
  .mic-tone-warn   { border-left-color: var(--warning); }
  .mic-tone-danger { border-left-color: var(--error);   }
  .mic-tone-safe   :global(.mic-row-icon) { color: var(--success); }
  .mic-tone-warn   :global(.mic-row-icon) { color: var(--warning); }
  .mic-tone-danger :global(.mic-row-icon) { color: var(--error);   }

  .mic-source code {
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: 3px;
    padding: 1px 4px;
    font-family: var(--font-mono, monospace);
    font-size: 10.5px;
  }

  /* Footer now uses the shared `<ModalFooter>` widget — no overrides needed. */
</style>
