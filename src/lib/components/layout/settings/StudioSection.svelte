<script lang="ts">
  /**
   * Studio settings — host-wide tunables for the RON / JSON / TOML
   * sidebar. Today only the persistent index toggle lives here; future
   * additions (auto-rescan interval, max indexed file size, …) plug in
   * the same way.
   */

  import { Boxes, RefreshCw, Info, Database } from 'lucide-svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import { studioStore } from '$lib/stores/studio.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { onMount } from 'svelte';

  let saved     = $state(false);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let cfgLoaded = $state(false);

  // Local mirror of the settings; one-way sync from the store on mount,
  // then we treat the toggle as a controlled input that pushes back to
  // the store (and host) on change.
  let useIndex = $state(false);

  onMount(async () => {
    await studioStore.ensureSettingsLoaded();
    useIndex = studioStore.settings.use_index;
    cfgLoaded = true;
  });

  async function persist() {
    await studioStore.updateSettings({ use_index: useIndex });
    saved = true;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => { saved = false; }, 2000);
    // When the user just enabled the index, kick off the first build
    // for the active repo — otherwise the cache stays empty until the
    // next sidebar mount.
    if (useIndex) {
      await studioStore.installIndexListeners();
      const tabId = tabsStore.activeTabId;
      if (tabId && !studioStore.indexJobRunning) {
        void studioStore.refreshIndex(tabId);
      }
    }
  }

  function rebuildNow() {
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    void studioStore.installIndexListeners();
    void studioStore.refreshIndex(tabId);
  }

  const progress     = $derived(studioStore.indexProgress);
  const indexRunning = $derived(studioStore.indexJobRunning);
</script>

<div class="section-header">
  <h2>Studio</h2>
  <p>Project-wide RON / JSON / TOML sidebar and cross-reference index.</p>
</div>

<!-- ── Persistent index ─────────────────────────────────────────────────── -->
<div class="card">
  <div class="card-section-title"><Database size={12} /> Cross-Reference Index</div>

  <FormRow
    label="Enable persistent index"
    description="Cache every `.ron` file's top-level definitions and reference fields on disk so cross-reference and find-usages queries are near-instant on large repos. The index lives at .arbor/studio-index.json and is refreshed lazily in the background — never on the UI thread."
  >
    <Toggle bind:checked={useIndex} onchange={persist} ariaLabel="Enable persistent index" />
  </FormRow>

  {#if useIndex && cfgLoaded}
    <FormRow
      label="Rebuild now"
      description="Force a full re-walk of the active repo. Useful after editing `.ron` files outside of Arbor or changing the reference-field convention in `.ron-studio.toml`."
    >
      <button class="btn-ghost"
              onclick={rebuildNow}
              disabled={indexRunning || !tabsStore.activeTabId}>
        <RefreshCw size={11} class={indexRunning ? 'spin' : ''} />
        <span>{indexRunning ? 'Indexing…' : 'Rebuild index'}</span>
      </button>
      {#if progress}
        <span class="progress-chip">
          {progress.processed}/{progress.total} files
        </span>
      {/if}
    </FormRow>
  {/if}
</div>

<div class="info-box">
  <Info size={13} />
  <span>
    Without the index, every cross-reference scan re-walks the repo and
    re-parses every <code>.ron</code> file from disk. Sub-100-file repos won't
    notice — turn it on when find-usages starts to feel sluggish (typically
    around 200+ files). The index is rebuilt automatically every time
    you save inside RON Studio.
  </span>
</div>

{#if saved}
  <p class="saved-label">Saved</p>
{/if}

<style>
  .progress-chip {
    margin-left: 8px;
    padding: 2px 8px;
    font-size: 11px;
    font-family: var(--font-code);
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: 10px;
    color: var(--text-secondary);
  }
</style>
