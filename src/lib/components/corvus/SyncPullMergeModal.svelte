<script lang="ts">
  import { RefreshCw, FolderGit2, SlidersHorizontal, Boxes, Database, TriangleAlert, GitBranch } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import { syncPullPreview, syncPullApply } from '$lib/ipc/corvus/sync';
  import type { PullPlan, PullSelections } from '$lib/types/corvus/sync';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { installPlugin, setPluginEnabled } from '$lib/ipc/marketplace';
  import { workspacesStore } from '$lib/stores/corvus/workspaces.svelte';
  import Modal from './../shared/Modal.svelte';
  import ModalHeader from './../shared/ModalHeader.svelte';
  import Button from './../shared/ui/Button.svelte';
  import RadioGroup from './../shared/ui/RadioGroup.svelte';
  import Toggle from './../shared/ui/Toggle.svelte';
  import Spinner from './../shared/ui/Spinner.svelte';

  let { onClose }: { onClose: () => void } = $props();

  type Choice = 'local' | 'remote';
  const CHOICES = [
    { value: 'local', label: 'Keep local' },
    { value: 'remote', label: 'Use remote' },
  ];

  let plan     = $state<PullPlan | null>(null);
  let loading  = $state(true);
  let applying = $state(false);
  let error    = $state<string | null>(null);

  // Per-item choices (reassigned wholesale so runes react).
  let wsChoice   = $state<Record<string, Choice>>({});
  let setChoice  = $state<Record<string, Choice>>({});
  let dataChoice = $state<Record<string, Choice>>({});
  // Install any missing mods and apply their enable state (via the marketplace).
  let modsApply  = $state(true);

  onMount(load);

  async function load() {
    loading = true; error = null;
    try {
      const p = await syncPullPreview();
      plan = p;
      // Preselect: import anything new/changed; leave unchanged items local.
      const ws: Record<string, Choice> = {};
      for (const w of p.workspaces) ws[w.id] = w.status === 'same' ? 'local' : 'remote';
      wsChoice = ws;
      const set: Record<string, Choice> = {};
      for (const s of p.settings) set[s.key] = s.differs ? 'remote' : 'local';
      setChoice = set;
      const data: Record<string, Choice> = {};
      for (const d of p.plugin_data) data[d.name] = d.differs ? 'remote' : 'local';
      dataChoice = data;
    } catch (e) {
      error = `${e}`;
    } finally {
      loading = false;
    }
  }

  const modsToApply = $derived(modsApply ? (plan?.mods.length ?? 0) : 0);
  const selectedCount = $derived(
    Object.values(wsChoice).filter(c => c === 'remote').length +
    Object.values(setChoice).filter(c => c === 'remote').length +
    Object.values(dataChoice).filter(c => c === 'remote').length +
    modsToApply
  );

  async function apply() {
    if (applying) return;
    applying = true;
    const sel: PullSelections = {
      workspace_ids:     Object.entries(wsChoice).filter(([, c]) => c === 'remote').map(([id]) => id),
      settings_keys:     Object.entries(setChoice).filter(([, c]) => c === 'remote').map(([k]) => k),
      plugin_data_names: Object.entries(dataChoice).filter(([, c]) => c === 'remote').map(([n]) => n),
    };
    try {
      // 1) Backend writes the files it owns (workspaces, settings, plugin data).
      const summary = await syncPullApply(sel);

      // 2) Mods go through the marketplace so the install ledger + plugin host
      //    stay authoritative: install any missing one, then apply its enable state.
      let modsDone = 0;
      let modsFailed = 0;
      if (modsApply && plan) {
        for (const m of plan.mods) {
          try {
            if (!m.installed) await installPlugin(m.name);
            await setPluginEnabled(m.name, m.enabled);
            modsDone++;
          } catch {
            modsFailed++;
          }
        }
      }

      // 3) Refresh so the changes are visible immediately (the sidebar reads a
      //    reactive store, not the file the backend just wrote). `load()` reloads
      //    both the workspaces and the repo registry.
      await workspacesStore.load();
      if (summary.settings_applied > 0) {
        window.dispatchEvent(new CustomEvent('arbor:sync-settings-applied'));
      }

      const parts: string[] = [];
      if (summary.workspaces_applied) parts.push(`${summary.workspaces_applied} workspace(s)`);
      if (summary.settings_applied) parts.push(`${summary.settings_applied} settings group(s)`);
      if (modsDone) parts.push(`${modsDone} mod(s)`);
      if (summary.plugin_data_applied) parts.push(`${summary.plugin_data_applied} plugin data`);
      let msg = parts.length ? `Applied ${parts.join(', ')}` : 'Nothing to apply';
      if (modsFailed) msg += ` · ${modsFailed} mod(s) couldn't be installed`;
      uiStore.showToast(msg, modsFailed ? 'warning' : 'success');
      onClose();
    } catch (e) {
      uiStore.showToast(`Pull failed: ${e}`, 'error');
    } finally {
      applying = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey) && !applying) apply();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<Modal {onClose} width="660px" height="580px" ariaLabel="Pull settings from sync repo">
  {#snippet header()}
    <ModalHeader {onClose}>
      <RefreshCw size={14} />
      <span class="modal-title">Pull &amp; merge settings</span>
    </ModalHeader>
  {/snippet}

  <div class="body">
    {#if loading}
      <div class="state"><Spinner /> Reading remote…</div>
    {:else if error}
      <div class="state err">{error}</div>
    {:else if !plan || !plan.available}
      <div class="state">The sync repo has no bundle yet — push from this or another machine first.</div>
    {:else}
      <p class="lead">Choose what to bring over from the remote. Unchanged items default to
        keeping your local copy.</p>

      {#if plan.workspaces.length}
        <section>
          <div class="group-title"><FolderGit2 size={12} /> Workspaces</div>
          {#each plan.workspaces as w (w.id)}
            <div class="row">
              <div class="row-main">
                <span class="row-name">{w.name}</span>
                <span class="row-sub">{w.repo_count} repo(s) · {w.status}</span>
              </div>
              <RadioGroup
                value={wsChoice[w.id]}
                options={CHOICES}
                appearance="segment"
                size="sm"
                onchange={(v) => (wsChoice = { ...wsChoice, [w.id]: v as Choice })}
              />
            </div>
          {/each}
        </section>
      {/if}

      {#if plan.settings.length}
        <section>
          <div class="group-title"><SlidersHorizontal size={12} /> Settings</div>
          {#each plan.settings as s (s.key)}
            <div class="row">
              <div class="row-main">
                <span class="row-name">{s.label}</span>
                <span class="row-sub">{s.differs ? 'differs' : 'identical'}</span>
              </div>
              <RadioGroup
                value={setChoice[s.key]}
                options={CHOICES}
                appearance="segment"
                size="sm"
                onchange={(v) => (setChoice = { ...setChoice, [s.key]: v as Choice })}
              />
            </div>
          {/each}
        </section>
      {/if}

      {#if plan.mods.length}
        <section>
          <div class="group-title"><Boxes size={12} /> Mods</div>
          <div class="row">
            <div class="row-main">
              <span class="row-name">Install missing &amp; apply enable state</span>
              <span class="row-sub">Missing mods are installed from the marketplace</span>
            </div>
            <Toggle checked={modsApply} onchange={(v) => (modsApply = v)} />
          </div>
          <div class="mod-list">
            {#each plan.mods as m (m.name)}
              <span class="mod-chip" class:missing={!m.installed}>
                {m.name}{#if !m.installed}<span class="mod-tag">install</span>{/if}
              </span>
            {/each}
          </div>
        </section>
      {/if}

      {#if plan.plugin_data.length}
        <section>
          <div class="group-title"><Database size={12} /> Plugin data</div>
          {#each plan.plugin_data as d (d.name)}
            <div class="row">
              <div class="row-main">
                <span class="row-name">{d.name}</span>
                <span class="row-sub">{d.differs ? 'differs' : 'identical'}</span>
              </div>
              <RadioGroup
                value={dataChoice[d.name]}
                options={CHOICES}
                appearance="segment"
                size="sm"
                onchange={(v) => (dataChoice = { ...dataChoice, [d.name]: v as Choice })}
              />
            </div>
          {/each}
        </section>
      {/if}

      {#if plan.missing_repos.length}
        <section>
          <div class="group-title"><TriangleAlert size={12} /> Repos not on this machine</div>
          <div class="hint">These are referenced by synced workspaces but not cloned here — you can clone or locate them after applying.</div>
          {#each plan.missing_repos as r (r.remote_url)}
            <div class="missing-row">
              <GitBranch size={11} />
              <span class="missing-name">{r.display_name}</span>
              <span class="missing-url">{r.remote_url}</span>
            </div>
          {/each}
        </section>
      {/if}
    {/if}
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={onClose} disabled={applying}>Cancel</Button>
    <Button
      variant="primary"
      onclick={apply}
      disabled={applying || loading || !plan?.available}
      loading={applying}
    >
      {applying ? 'Applying…' : (selectedCount ? `Apply ${selectedCount}` : 'Apply')}
    </Button>
  {/snippet}
</Modal>

<style>
  .body { display: flex; flex-direction: column; gap: 14px; }
  .lead { margin: 0; font-size: var(--font-size-sm); color: var(--text-secondary); line-height: 1.4; }

  section { display: flex; flex-direction: column; gap: 6px; }
  .group-title {
    display: inline-flex; align-items: center; gap: 5px;
    font-size: var(--font-size-xs); font-weight: 600; color: var(--text-secondary);
    text-transform: uppercase; letter-spacing: 0.5px;
  }

  .row {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    padding: 7px 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
  }
  .row-main { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .row-name { font-size: var(--font-size-sm); color: var(--text-primary); }
  .row-sub { font-size: var(--font-size-2xs); color: var(--text-muted); }

  .mod-list { display: flex; flex-wrap: wrap; gap: 4px; padding: 2px 0; }
  .mod-chip {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 2px 7px; font-size: var(--font-size-2xs);
    background: var(--bg-elevated); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); color: var(--text-secondary);
    font-family: var(--font-code);
  }
  .mod-chip.missing { color: var(--text-muted); }
  .mod-tag {
    font-size: var(--font-size-3xs); text-transform: uppercase; letter-spacing: 0.3px;
    color: var(--accent); background: var(--accent-subtle);
    padding: 0 4px; border-radius: 999px;
  }

  .hint { font-size: var(--font-size-2xs); color: var(--text-muted); line-height: 1.4; }

  .missing-row {
    display: flex; align-items: center; gap: 6px;
    font-size: var(--font-size-xs); color: var(--text-secondary);
    padding: 3px 2px;
  }
  .missing-name { color: var(--text-primary); }
  .missing-url { color: var(--text-muted); font-family: var(--font-code); font-size: var(--font-size-2xs); }

  .state {
    display: flex; align-items: center; gap: 8px; justify-content: center;
    padding: 40px 20px; color: var(--text-muted); font-size: var(--font-size-sm);
  }
  .state.err { color: var(--color-error, #e06c75); }
</style>
