<script lang="ts">
  /**
   * Bennu footer — the IntelliJ-style status strip for the Java-editor window.
   * Left: JDK (version + where it was resolved from) · detected capabilities count.
   * Right: indexing status · the open file's encoding · caret Ln/Col · the
   * shared feedback badges (jobs · notifications), injected by the window via the
   * `footerExtra` snippet so this file stays free of Arbor feedback-store imports.
   *
   * bg-elevated strip (flows from the titlebar) — mirrors MerulaFooter / Corvus
   * StatusBar. Subtle + keyboard-first (nothing here is mouse-only).
   */
  import { Coffee, Boxes, Database, FileType } from 'lucide-svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuIndexStore } from '$lib/stores/bennu/index.svelte';
  import type { Snippet } from 'svelte';

  let { footerExtra }: { footerExtra?: Snippet } = $props();

  const jdk = $derived(projectStore.project?.jdk ?? null);
  const encoding = $derived(projectStore.activeEncoding);

  // Detected capability count (the truthy boolean fields on the set).
  const capCount = $derived.by(() => {
    const c = projectStore.capabilities;
    if (!c) return 0;
    let n = 0;
    for (const [k, v] of Object.entries(c)) { if (k !== 'hits' && v === true) n++; }
    return n;
  });

  // Human-readable source label for the JDK tooltip.
  const jdkSourceLabel: Record<string, string> = {
    'maven.compiler.source': 'from maven.compiler.source',
    'maven.compiler.target': 'from maven.compiler.target',
    'compiler-plugin': 'from the compiler plugin',
    'toolchains': 'from toolchains',
    'override': 'overridden manually',
    'default': 'default (not inferred)',
  };
</script>

<div class="bf">
  {#if projectStore.project}
    {#if jdk}
      <span class="bf-item" use:tooltip={`JDK ${jdk.version} · ${jdkSourceLabel[jdk.source] ?? jdk.source}`}>
        <Coffee size={12} /> JDK {jdk.version}
        <span class="bf-sub">{jdk.source}</span>
      </span>
    {:else}
      <span class="bf-item bf-muted" use:tooltip={'JDK not inferred — set an override'}>
        <Coffee size={12} /> JDK —
      </span>
    {/if}

    <span class="bf-sep"></span>

    <span class="bf-item" use:tooltip={`${capCount} domain capabilit${capCount === 1 ? 'y' : 'ies'} detected`}>
      <Boxes size={12} /> {capCount} capabilit{capCount === 1 ? 'y' : 'ies'}
    </span>
  {:else}
    <span class="bf-item bf-muted">No project open</span>
  {/if}

  <span class="bf-spacer"></span>

  {#if projectStore.project}
    <!-- Indexing status — driven by the real index-progress events / stats poll. -->
    {#if bennuIndexStore.indexing}
      {@const rp = bennuIndexStore.refProgress}
      <span class="bf-item bf-indexing" use:tooltip={`Building the project index${bennuIndexStore.phaseLabel ? ` · ${bennuIndexStore.phaseLabel}` : ''}`}>
        <Spinner size={11} /> Indexing{bennuIndexStore.phaseLabel ? ` ${bennuIndexStore.phaseLabel.toLowerCase()}` : ''}{rp ? ` ${rp.done.toLocaleString()}/${rp.total.toLocaleString()}` : ''}…
      </span>
    {:else}
      <span class="bf-item" use:tooltip={bennuIndexStore.typeCount ? `Index ready · ${bennuIndexStore.typeCount} types` : 'Project index is up to date'}>
        <Database size={12} /> Indexed{bennuIndexStore.typeCount ? ` · ${bennuIndexStore.typeCount}` : ''}
      </span>
    {/if}

    {#if encoding}
      <span class="bf-sep"></span>
      <span class="bf-item" use:tooltip={'File encoding'}>
        <FileType size={12} /> {encoding}
      </span>
    {/if}
    <!-- Caret Ln/Col lives on the editor's own footer (BennuEditor); not duplicated here. -->
  {/if}

  {#if footerExtra}
    <span class="bf-sep"></span>
    {@render footerExtra()}
  {/if}
</div>

<style>
  .bf {
    display: flex; align-items: center; gap: 10px;
    height: 24px; flex-shrink: 0;
    padding: 0 12px;
    background: var(--bg-elevated);
    border-top: 1px solid var(--border-subtle);
    font-size: 11px; color: var(--text-muted);
    user-select: none;
  }
  .bf-item { display: flex; align-items: center; gap: 4px; white-space: nowrap; }
  .bf-item :global(svg) { color: var(--text-disabled); }
  .bf-indexing { color: var(--accent); }
  .bf-indexing :global(svg) { color: var(--accent); }
  .bf-muted { color: var(--text-disabled); }
  .bf-sub {
    font-size: 10px; color: var(--text-disabled);
    padding-left: 2px; max-width: 160px; overflow: hidden; text-overflow: ellipsis;
  }
  .bf-spacer { flex: 1; }
  .bf-sep { width: 1px; height: 12px; background: var(--border-subtle); flex-shrink: 0; }
</style>
