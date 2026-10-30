<script lang="ts">
  /**
   * The profiler tool window (bottom dock): read a recording as a flame graph, a
   * hot-methods table, allocations and GC, lock contention, or a comparison with an earlier one.
   * The same numbers are drawn in the editor, on the methods they belong to (Settings › Editor ›
   * Profiler hints).
   */
  import { Eye, EyeOff, Flame, FileUp, Gauge, RefreshCw, SlidersHorizontal, Trash2 } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuProfilerStore as store, type ProfilerTab } from '$lib/stores/bennu/profiler.svelte';
  import ProfilerCompareTab from './ProfilerCompareTab.svelte';
  import ProfilerFlameTab from './ProfilerFlameTab.svelte';
  import ProfilerLocksTab from './ProfilerLocksTab.svelte';
  import ProfilerMemoryTab from './ProfilerMemoryTab.svelte';
  import ProfilerMethodsTab from './ProfilerMethodsTab.svelte';
  import ProfilerStatusBar from './ProfilerStatusBar.svelte';
  import { fmtMs } from './profiler-format';

  let picking = $state(false);

  let lastRoot: string | null = null;
  $effect(() => {
    const root = projectStore.project?.root ?? null;
    if (root !== lastRoot) {
      lastRoot = root;
      void store.loadDetect();
      void store.loadPrefs();
      void store.refresh();
    }
  });

  function onPanelKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'f' && store.hasRecordings) {
      e.preventDefault();
      e.stopPropagation();
      store.requestSearchFocus();
    }
  }

  const meta = $derived(store.activeMeta);
  const recordingOptions = $derived(
    store.recordings.map((m) => ({ value: m.id, label: `${m.label} — ${new Date(m.created_ms).toLocaleString()}` })),
  );
  const tabs = $derived([
    { id: 'flame', label: 'Flame graph' },
    { id: 'methods', label: 'Hot methods' },
    { id: 'memory', label: 'Memory' },
    { id: 'locks', label: 'Waits' },
    { id: 'compare', label: 'Compare' },
  ]);
  const summary = $derived(
    meta
      ? `${meta.samples.toLocaleString()} samples · ${meta.duration_ms ? fmtMs(meta.duration_ms) : ''} · hottest: ${meta.top_method || '—'}`
      : '',
  );
</script>

<!-- Ctrl+F is the search of whatever is on screen: inside this panel it is the profiler's, not the editor's. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="prof" onkeydown={onPanelKey}>
  <BottomPanelHeader title="Profiler" count={store.recordings.length} onClose={() => bennuUiStore.closeBottom()}>
    {#snippet icon()}<Flame size={13} />{/snippet}
    {#snippet actions()}
      <!-- The numbers drawn in the editor come from the recording shown here; once they have been read they are
           noise on the code, so they go away from here as well as from Settings and the palette. -->
      <IconButton
        tooltip={bennuSettingsStore.profilerHints ? 'Hide the numbers in the editor' : 'Show the numbers in the editor'}
        size={22}
        active={bennuSettingsStore.profilerHints}
        onclick={() => bennuSettingsStore.setProfilerHints(!bennuSettingsStore.profilerHints)}
      >
        {#if bennuSettingsStore.profilerHints}<Eye size={13} />{:else}<EyeOff size={13} />{/if}
      </IconButton>
      <IconButton tooltip="Profile… — a command, a running JVM or Java with its own settings" size={22} disabled={store.phase !== 'idle'} onclick={() => store.openStart()}>
        <Gauge size={13} />
      </IconButton>
      <IconButton tooltip="Import a recording (.jfr, .cpuprofile, samply .json / .json.gz)" size={22} onclick={() => (picking = true)}>
        <FileUp size={13} />
      </IconButton>
      <IconButton tooltip="Colour thresholds (yellow / orange / red)" size={22} onclick={() => store.openThresholds()}>
        <SlidersHorizontal size={13} />
      </IconButton>
      <IconButton tooltip="Refresh" size={22} onclick={() => { void store.loadDetect(); void store.refresh(); }}>
        <RefreshCw size={13} />
      </IconButton>
      <IconButton tooltip="Delete this recording" size={22} variant="danger" disabled={!meta} onclick={() => meta && void store.remove(meta.id)}>
        <Trash2 size={13} />
      </IconButton>
    {/snippet}
  </BottomPanelHeader>

  <ProfilerStatusBar />

  {#if store.recordings.length > 0}
    <div class="sel">
      <Select size="sm" value={store.activeId ?? ''} options={recordingOptions} onchange={(id) => void store.select(id)} />
      <Tabs
        size="sm"
        variant="underline"
        value={store.tab}
        ariaLabel="Profiler views"
        items={tabs}
        onSelect={(id) => store.setTab(id as ProfilerTab)}
      />
      <span class="sum">{summary}</span>
    </div>
    {#if store.view && store.view.notes.length > 0}
      <div class="notes">{store.view.notes.join(' ')}</div>
    {/if}
    <div class="body">
      {#if store.loadingView}
        <div class="state"><Spinner size={13} /> Loading…</div>
      {:else if store.tab === 'flame'}
        <ProfilerFlameTab />
      {:else if store.tab === 'methods'}
        <ProfilerMethodsTab />
      {:else if store.tab === 'memory'}
        <ProfilerMemoryTab />
      {:else if store.tab === 'locks'}
        <ProfilerLocksTab />
      {:else}
        <ProfilerCompareTab />
      {/if}
    </div>
  {:else if store.phase === 'idle'}
    <div class="empty">
      <EmptyState
        message="No recordings yet"
        description="Record a run with the gauge button in the title bar (or Profile… for a command or a running JVM), or import a .jfr / .cpuprofile. The numbers then appear next to the methods in the editor."
      />
    </div>
  {/if}
</div>

{#if picking}
  <FileExplorerModal
    mode="file"
    title="Import a recording"
    extensions={['jfr', 'cpuprofile', 'json', 'gz']}
    initialPath={projectStore.project?.root}
    onConfirm={(file) => { picking = false; void store.importFile(file); }}
    onCancel={() => (picking = false)}
    onClose={() => (picking = false)}
  />
{/if}

<style>
  .prof { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--bg-base); }
  .sel { display: flex; align-items: center; gap: 10px; padding: 2px 8px; border-bottom: 1px solid var(--border-subtle, var(--border)); }
  .sum { margin-left: auto; font-size: var(--font-size-xs); color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .notes { padding: 3px 10px; font-size: var(--font-size-xs); color: var(--warning); }
  .body { display: flex; flex-direction: column; flex: 1; min-height: 0; }
  .empty, .state { display: flex; align-items: center; justify-content: center; gap: 8px; flex: 1; color: var(--text-muted); }
</style>
