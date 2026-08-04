<script lang="ts">
  /**
   * Bennu footer — the IntelliJ-style status strip for the editor window.
   * Left: JDK (version + where it was resolved from) · detected capabilities count, or
   * the crate count on a Cargo project.
   * Right: indexing status · the open file's encoding · caret Ln/Col · the
   * shared feedback badges (jobs · notifications), injected by the window via the
   * `footerExtra` snippet so this file stays free of Arbor feedback-store imports.
   *
   * On a **Cargo** project the Java facts are replaced rather than blanked: JDK,
   * capabilities and the index are all Java-model readings that don't exist for a Rust
   * root (see `bennu_open_project`), and `JDK —` next to `0 capabilities` reads as a
   * broken Java project instead of a Rust one. The strip states what it does know: the
   * toolchain it is, and how many crates the workspace holds.
   *
   * bg-elevated strip (flows from the titlebar) — mirrors MerulaFooter / Corvus
   * StatusBar. Subtle + keyboard-first (nothing here is mouse-only).
   */
  import { Coffee, Boxes, Database, FileType, Package } from 'lucide-svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import BennuIndentStatus from './BennuIndentStatus.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuIndexStore } from '$lib/stores/bennu/index.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import type { Snippet } from 'svelte';

  let { footerExtra }: { footerExtra?: Snippet } = $props();

  const isCargo = $derived(projectStore.isCargo);
  const jdk = $derived(projectStore.project?.jdk ?? null);
  /** Crates in a Cargo workspace: the expanded `members`, plus the root crate itself when
   *  the manifest declares a `[package]` (a virtual workspace manifest has none, and then
   *  the members ARE the whole project). */
  const crateCount = $derived((projectStore.project?.modules.length ?? 0) || 1);
  // The PROJECT's declared source encoding (pom `sourceEncoding` → config default). The
  // open file's own decoded encoding lives on the editor's footer (BennuEditor), which
  // can differ (per-file override / recovered mislabel).
  const encoding = $derived(projectStore.project?.source_encoding ?? null);

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
    {#if isCargo}
      <span class="bf-item" use:tooltip={'A Cargo project — editor features only (no symbol index yet)'}>
        <Package size={12} /> Cargo
      </span>
      <span class="bf-sep"></span>
      <span class="bf-item" use:tooltip={'Crates in this workspace (Cargo.toml members)'}>
        <Boxes size={12} /> {crateCount} crate{crateCount === 1 ? '' : 's'}
      </span>
    {:else if jdk}
      <span class="bf-item" use:tooltip={`JDK ${jdk.version} · ${jdkSourceLabel[jdk.source] ?? jdk.source}`}>
        <Coffee size={12} /> JDK {jdk.version}
        <span class="bf-sub">{jdk.source}</span>
      </span>
    {:else}
      <span class="bf-item bf-muted" use:tooltip={'JDK not inferred — set an override'}>
        <Coffee size={12} /> JDK —
      </span>
    {/if}

    {#if !isCargo}
      <span class="bf-sep"></span>

      <span class="bf-item" use:tooltip={`${capCount} domain capabilit${capCount === 1 ? 'y' : 'ies'} detected`}>
        <Boxes size={12} /> {capCount} capabilit{capCount === 1 ? 'y' : 'ies'}
      </span>
    {/if}
  {:else}
    <span class="bf-item bf-muted">No project open</span>
  {/if}

  <span class="bf-spacer"></span>

  <!-- Go-to in progress. Only shown once it has taken long enough to be worth saying
       (the store holds it back), and it is the only feedback there is: until the target
       opens, nothing else on screen changes. -->
  {#if bennuUiStore.navigatingTo}
    <span class="bf-item bf-navigating" use:tooltip={'Resolving the declaration — a library type is read from the classpath'}>
      <Spinner size={11} /> Opening {bennuUiStore.navigatingTo}…
    </span>
    <span class="bf-sep"></span>
  {/if}

  {#if projectStore.project}
    <!-- Indexing status — driven by the real index-progress events / stats poll. A Cargo
         project builds no index, so "Indexed · 0" would be a reading of nothing. -->
    {#if !isCargo}
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
      <span class="bf-sep"></span>
    {/if}

    <!-- Indentation (tabs/spaces + width) — click / keyboard to change; applies live. -->
    <BennuIndentStatus />

    {#if encoding}
      <span class="bf-sep"></span>
      <span class="bf-item" use:tooltip={isCargo ? 'Rust source is UTF-8 by language definition' : 'Project source encoding (pom sourceEncoding)'}>
        <FileType size={12} /> {encoding}
      </span>
    {/if}
    <!-- The open file's own encoding + caret Ln/Col live on the editor's footer (BennuEditor). -->
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
    font-size: var(--font-size-xs); color: var(--text-muted);
    user-select: none;
  }
  .bf-item { display: flex; align-items: center; gap: 4px; white-space: nowrap; }
  .bf-item :global(svg) { color: var(--text-disabled); }
  .bf-indexing { color: var(--accent); }
  .bf-indexing :global(svg) { color: var(--accent); }
  .bf-navigating { color: var(--accent); }
  .bf-navigating :global(svg) { color: var(--accent); }
  .bf-muted { color: var(--text-disabled); }
  .bf-sub {
    font-size: var(--font-size-2xs); color: var(--text-disabled);
    padding-left: 2px; max-width: 160px; overflow: hidden; text-overflow: ellipsis;
  }
  .bf-spacer { flex: 1; }
  .bf-sep { width: 1px; height: 12px; background: var(--border-subtle); flex-shrink: 0; }
</style>
