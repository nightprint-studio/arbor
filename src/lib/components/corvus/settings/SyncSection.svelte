<script lang="ts">
  import { RefreshCw, Github, Check, TriangleAlert, Clock, Boxes, Cloud } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import { syncStore } from '$lib/stores/corvus/sync.svelte';
  import { gitProviders } from '$lib/ipc/corvus/providers';
  import type { SyncConfig } from '$lib/types/corvus/sync';
  import { uiStore } from '$lib/stores/ui.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';

  let connected = $state(false);
  let provider  = $state<'github'>('github');
  let repoName  = $state('');
  let busy      = $state(false);
  let saved     = $state(false);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;

  const cfg    = $derived(syncStore.config);
  const status = $derived(syncStore.status);

  onMount(async () => {
    await syncStore.loadConfig();
    try {
      const s = await gitProviders.authStatus('github');
      connected = s.authenticated;
    } catch { connected = false; }
  });

  function flashSaved() {
    saved = true;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => { saved = false; }, 1800);
  }

  async function patch(part: Partial<SyncConfig>) {
    const base = syncStore.config;
    if (!base) return;
    try {
      await syncStore.saveConfig({ ...base, ...part });
      flashSaved();
    } catch (e) {
      uiStore.showToast(`Save failed: ${e}`, 'error');
    }
  }

  async function enable() {
    if (busy) return;
    busy = true;
    try {
      await syncStore.enable(provider, repoName.trim() || null);
      uiStore.showToast('Settings sync enabled — first push done', 'success');
    } catch (e) {
      uiStore.showToast(`Could not enable sync: ${e}`, 'error');
    } finally {
      busy = false;
    }
  }

  async function disable() {
    busy = true;
    try { await syncStore.disable(); }
    catch (e) { uiStore.showToast(`Could not disable sync: ${e}`, 'error'); }
    finally { busy = false; }
  }

  async function pushNow() {
    if (busy) return;
    busy = true;
    try {
      await syncStore.pushNow();
      uiStore.showToast('Pushed corvus settings', 'success');
    } catch (e) {
      uiStore.showToast(`Push failed: ${e}`, 'error');
    } finally {
      busy = false;
    }
  }

  function openPull() {
    // Dispatch first (AppShell mounts the modal), then close the settings panel
    // so the modal isn't rendered behind the settings overlay.
    window.dispatchEvent(new CustomEvent('arbor:open-sync-pull'));
    uiStore.setPanel('graph');
  }

  function fmtTime(ts: number | null | undefined): string {
    if (!ts) return 'never';
    return new Date(ts * 1000).toLocaleString();
  }
</script>

<div class="section-header">
  <h2>Settings Sync</h2>
  <p>Mirror your corvus workspaces, settings, installed-mod list and light plugin
     data to a <strong>private GitHub repo</strong>, so a new machine picks up where
     you left off. Repository paths, credentials and heavy caches/indexes are never
     synced.</p>
</div>

{#if !connected}
  <div class="card notice">
    <TriangleAlert size={13} />
    <div>
      <div class="notice-title">Connect GitHub first</div>
      <div class="notice-body">Settings sync needs a connected GitHub account.
        Open <em>Access → Git</em> and connect GitHub, then come back here.</div>
    </div>
  </div>
{/if}

{#if cfg && !cfg.enabled}
  <!-- Enable -->
  <div class="card">
    <div class="card-section-title"><Cloud size={12} /> Enable sync</div>

    <FormRow label="Provider" description="Where the private sync repo lives.">
      <div class="provider-pill"><Github size={13} /> GitHub</div>
    </FormRow>

    <FormRow
      label="Repository name"
      description="Leave blank to use (or adopt) the default 'arbor-corvus-sync'. Created private if it doesn't exist yet."
    >
      <input
        class="row-input"
        type="text"
        placeholder="arbor-corvus-sync (auto)"
        bind:value={repoName}
        disabled={!connected || busy}
        onkeydown={(e) => { if (e.key === 'Enter') enable(); }}
      />
    </FormRow>

    <div class="actions">
      <Button variant="primary" onclick={enable} disabled={!connected || busy} loading={busy}>
        <Cloud size={13} /> Enable & push
      </Button>
    </div>
  </div>
{:else if cfg && status}
  <!-- Active -->
  <div class="card">
    <div class="card-section-title"><Check size={12} /> Connected</div>
    <div class="repo-row">
      <Github size={14} />
      <span class="repo-name">{status.repo_full_name ?? '—'}</span>
      <span class="badge private">private</span>
      {#if status.dirty}<span class="badge dirty">changes pending</span>{/if}
    </div>
    <div class="meta">
      <span><Clock size={11} /> Last push: {fmtTime(status.last_push_at)}</span>
      <span><Clock size={11} /> Last pull: {fmtTime(status.last_pull_at)}</span>
    </div>

    {#if status.awaiting_pull}
      <div class="pull-banner">
        <TriangleAlert size={13} />
        <div>
          <div class="pull-banner-title">This repo already has settings from another machine</div>
          <div class="pull-banner-body">Pull &amp; merge to import them. Auto-push is paused until you do, so this
            machine can't overwrite the other's data.</div>
        </div>
      </div>
    {/if}

    <div class="actions">
      {#if status.awaiting_pull}
        <Button variant="primary" onclick={openPull} disabled={busy}>
          <RefreshCw size={12} /> Pull &amp; merge…
        </Button>
        <Button variant="ghost" onclick={pushNow} disabled={busy} loading={busy}>Push local anyway</Button>
      {:else}
        <Button variant="primary" onclick={pushNow} disabled={busy} loading={busy}>
          <RefreshCw size={12} /> Push now
        </Button>
        <Button variant="secondary" onclick={openPull} disabled={busy}>
          <RefreshCw size={12} /> Pull &amp; merge…
        </Button>
      {/if}
      <Button variant="ghost" onclick={disable} disabled={busy}>Disable</Button>
    </div>
  </div>

  <!-- Cadence -->
  <div class="card">
    <div class="card-section-title"><Clock size={12} /> Auto-push</div>
    <FormRow
      label="Push interval (seconds)"
      description="Minimum time between automatic pushes. Changes are batched and only pushed when something actually differs."
    >
      <NumberStepper
        value={cfg.interval_secs}
        min={30}
        step={30}
        ariaLabel="Push interval (seconds)"
        onchange={(v) => patch({ interval_secs: Math.max(30, Math.trunc(v ?? 300)) })}
      />
    </FormRow>
  </div>

  <!-- What to sync -->
  <div class="card">
    <div class="card-section-title"><Boxes size={12} /> What to sync</div>
    <FormRow label="Workspaces & repos" description="Repos are matched by remote URL, never by path.">
      <Toggle checked={cfg.include_workspaces} onchange={(v) => patch({ include_workspaces: v })} />
    </FormRow>
    <FormRow label="Settings" description="UI (theme, keybindings, animations) and corvus git preferences.">
      <Toggle checked={cfg.include_settings} onchange={(v) => patch({ include_settings: v })} />
    </FormRow>
    <FormRow label="Mod list" description="Installed plugin names, versions and enable state (re-installed from the marketplace on pull).">
      <Toggle checked={cfg.include_mods} onchange={(v) => patch({ include_mods: v })} />
    </FormRow>
    <FormRow label="Light plugin data" description="Each plugin's small global settings (e.g. compile/run commands). Heavy blobs are skipped.">
      <Toggle checked={cfg.include_plugin_data} onchange={(v) => patch({ include_plugin_data: v })} />
    </FormRow>
  </div>

  {#if saved}<div class="saved-pill"><Check size={11} /> Saved</div>{/if}
{/if}

<style>
  .section-header { margin-bottom: 14px; }
  .section-header h2 {
    font-size: var(--font-size-lg); font-weight: 600; margin: 0 0 3px;
    color: var(--text-primary); font-family: var(--font-ui-sans);
  }
  .section-header p {
    margin: 0; font-size: var(--font-size-sm); color: var(--text-secondary); line-height: 1.45;
    font-family: var(--font-ui-sans);
  }

  .card {
    padding: 10px 12px; margin-bottom: 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    font-family: var(--font-ui-sans);
  }
  .card-section-title {
    display: inline-flex; align-items: center; gap: 5px;
    font-size: var(--font-size-xs); font-weight: 600; color: var(--text-secondary);
    text-transform: uppercase; letter-spacing: 0.5px;
    margin-bottom: 8px;
  }

  .notice {
    display: flex; gap: 9px; align-items: flex-start;
    color: var(--warning);
    border-color: color-mix(in srgb, var(--warning) 40%, transparent);
    background: color-mix(in srgb, var(--warning) 8%, transparent);
  }
  .notice-title { font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary); }
  .notice-body { font-size: var(--font-size-xs); color: var(--text-secondary); line-height: 1.4; margin-top: 2px; }

  .pull-banner {
    display: flex; gap: 9px; align-items: flex-start;
    padding: 9px 11px; margin-bottom: 10px;
    color: var(--warning);
    border: 1px solid color-mix(in srgb, var(--warning) 40%, transparent);
    background: color-mix(in srgb, var(--warning) 8%, transparent);
    border-radius: var(--radius-sm);
  }
  .pull-banner-title { font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary); }
  .pull-banner-body { font-size: var(--font-size-xs); color: var(--text-secondary); line-height: 1.4; margin-top: 2px; }

  .provider-pill, .repo-row {
    display: inline-flex; align-items: center; gap: 6px;
    font-size: var(--font-size-sm); color: var(--text-primary);
  }
  .provider-pill {
    padding: 3px 10px; background: var(--bg-input);
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
  }
  .repo-row { gap: 8px; margin-bottom: 6px; }
  .repo-name { font-family: var(--font-code); font-size: var(--font-size-sm); color: var(--accent); }
  .badge {
    font-size: var(--font-size-3xs); text-transform: uppercase; letter-spacing: 0.4px;
    padding: 1px 6px; border-radius: 999px; font-weight: 600;
  }
  .badge.private { background: var(--bg-input); color: var(--text-muted); border: 1px solid var(--border-subtle); }
  .badge.dirty { background: color-mix(in srgb, var(--warning) 18%, transparent); color: var(--warning); }

  .meta {
    display: flex; flex-wrap: wrap; gap: 14px;
    font-size: var(--font-size-xs); color: var(--text-muted);
    margin-bottom: 10px;
  }
  .meta span { display: inline-flex; align-items: center; gap: 4px; }

  .row-input {
    padding: 4px 8px; font-size: var(--font-size-sm); color: var(--text-primary);
    background: var(--bg-input);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); outline: none;
    min-width: 200px; font-family: var(--font-ui-sans);
  }
  .row-input:focus { border-color: var(--accent); }
  .row-input:disabled { opacity: 0.55; }

  .actions {
    display: flex; flex-wrap: wrap; gap: 8px; align-items: center;
    margin-top: 10px;
  }
  .saved-pill {
    display: inline-flex; align-items: center; gap: 4px;
    font-size: var(--font-size-xs); color: var(--accent); margin-top: 2px;
  }
</style>
