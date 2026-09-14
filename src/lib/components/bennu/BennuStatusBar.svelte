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
   * Composed only of `StatusBarItem` / `StatusBarSeparator` — the look of a reading, and of a
   * reading that can be clicked, lives in those widgets. bg-elevated strip (flows from the
   * titlebar) — mirrors MerulaFooter / Corvus StatusBar. Keyboard-first: every actionable item is
   * a real button.
   */
  import {
    AlertTriangle, Coffee, Boxes, Database, FileType, Package, ServerCog, ServerCrash,
  } from 'lucide-svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StatusBarItem from '$lib/components/shared/ui/StatusBarItem.svelte';
  import StatusBarSeparator from '$lib/components/shared/ui/StatusBarSeparator.svelte';
  import BennuIndentStatus from './BennuIndentStatus.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuIndexStore } from '$lib/stores/bennu/index.svelte';
  import { bennuLspStore } from '$lib/stores/bennu/lsp.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { javaLevelStore } from '$lib/stores/bennu/java-level.svelte';
  import { fileOwnerStore } from '$lib/stores/bennu/file-owner.svelte';
  import type { Snippet } from 'svelte';

  let { footerExtra }: { footerExtra?: Snippet } = $props();

  /** The language server to report.
   *
   *  The file's own when it has one — a polyglot repo can have two up, and the one answering for
   *  what is on screen is the one worth naming — otherwise the project's. A server running for
   *  this project is a fact about the project, and hiding it while a `Cargo.toml` or a README is
   *  open removes the answer to "is it still indexing" exactly when that is the question. */
  const lsp = $derived(
    bennuLspStore.statusForProject(projectStore.project?.root, projectStore.activeFilePath),
  );
  /** Semantic tokens painted in the open buffer — surfaced in the tooltip because "the file is all
   *  white" has two indistinguishable causes, and this number tells them apart. */
  const tokens = $derived(bennuLspStore.tokenCount);

  const isCargo = $derived(projectStore.isCargo);
  const jdk = $derived(projectStore.project?.jdk ?? null);
  /** The open file's own module, when its pom declares a level of its own. A reactor part-way
   *  through a migration has one module on 21 and another still on 8, and the number that governs
   *  the checks on the file in front of you is that module's, not the project's. `null` — every
   *  single-module project — leaves the project's answer showing, exactly as before. */
  const moduleJdk = $derived(javaLevelStore.module);
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

  const openLanguageSettings = () => bennuUiStore.openSettings('languages');
</script>

<div class="bf">
  {#if projectStore.project}
    {#if isCargo}
      <StatusBarItem tooltip="A Cargo project — editor features only (no symbol index yet)">
        <Package size={12} /> Cargo
      </StatusBarItem>
      <StatusBarSeparator />
      <StatusBarItem tooltip="Crates in this workspace (Cargo.toml members)">
        <Boxes size={12} /> {crateCount} crate{crateCount === 1 ? '' : 's'}
      </StatusBarItem>
    {:else if moduleJdk}
      <StatusBarItem
        tooltip={`JDK ${moduleJdk.version} in module ${moduleJdk.module} · ${
          jdkSourceLabel[moduleJdk.source] ?? moduleJdk.source
        }${jdk && jdk.version !== moduleJdk.version ? ` — the project as a whole reads as JDK ${jdk.version}` : ''}`}
      >
        <Coffee size={12} /> JDK {moduleJdk.version}
        <span class="bf-sub">{moduleJdk.module}</span>
      </StatusBarItem>
    {:else if jdk}
      <StatusBarItem tooltip={`JDK ${jdk.version} · ${jdkSourceLabel[jdk.source] ?? jdk.source}`}>
        <Coffee size={12} /> JDK {jdk.version}
        <span class="bf-sub">{jdk.source}</span>
      </StatusBarItem>
    {:else}
      <StatusBarItem tone="muted" tooltip="JDK not inferred — set an override">
        <Coffee size={12} /> JDK —
      </StatusBarItem>
    {/if}

    {#if !isCargo}
      <StatusBarSeparator />
      <StatusBarItem tooltip={`${capCount} domain capabilit${capCount === 1 ? 'y' : 'ies'} detected`}>
        <Boxes size={12} /> {capCount} capabilit{capCount === 1 ? 'y' : 'ies'}
      </StatusBarItem>
    {/if}
  {:else}
    <StatusBarItem tone="muted">No project open</StatusBarItem>
  {/if}

  <span class="bf-spacer"></span>

  <!-- Go-to in progress. Only shown once it has taken long enough to be worth saying
       (the store holds it back), and it is the only feedback there is: until the target
       opens, nothing else on screen changes. -->
  {#if bennuUiStore.navigatingTo}
    <StatusBarItem tone="accent" tooltip="Resolving the declaration — a library type is read from the classpath">
      <Spinner size={11} /> Opening {bennuUiStore.navigatingTo}…
    </StatusBarItem>
    <StatusBarSeparator />
  {/if}

  {#if projectStore.project}
    <!-- Indexing status — driven by the real index-progress events / stats poll. A Cargo
         project builds no index, so "Indexed · 0" would be a reading of nothing. -->
    {#if !isCargo}
      <!-- The file on screen is outside every indexed project: only syntax checks run on it, and
           without this it reads as a file whose semantics were checked and found clean. -->
      {#if fileOwnerStore.notIndexed}
        {#if fileOwnerStore.suggestedRoot}
          <StatusBarItem
            tone="warning"
            tooltip={`This file is not under an indexed project — semantic checks are off. Click to ${
              fileOwnerStore.suggestionIsMember ? 'switch to' : 'open'
            } ${fileOwnerStore.suggestedRoot}`}
            onclick={() => void fileOwnerStore.openSuggested()}
          >
            <AlertTriangle size={12} /> Not indexed
          </StatusBarItem>
        {:else}
          <StatusBarItem tone="warning" tooltip="This file is not under any Maven project — semantic checks are off">
            <AlertTriangle size={12} /> Not indexed
          </StatusBarItem>
        {/if}
        <StatusBarSeparator />
      {/if}
      {#if bennuIndexStore.indexing}
        {@const rp = bennuIndexStore.refProgress}
        <StatusBarItem
          tone="accent"
          tooltip={`Building the project index${bennuIndexStore.phaseLabel ? ` · ${bennuIndexStore.phaseLabel}` : ''}`}
        >
          <Spinner size={11} /> Indexing{bennuIndexStore.phaseLabel ? ` ${bennuIndexStore.phaseLabel.toLowerCase()}` : ''}{rp ? ` ${rp.done.toLocaleString()}/${rp.total.toLocaleString()}` : ''}…
        </StatusBarItem>
      {:else if bennuIndexStore.failed}
        <StatusBarItem
          tone="warning"
          tooltip="The index could not be built — only syntax checks run. Click for the index inspector, where it can be rebuilt"
          onclick={() => bennuUiStore.openIndexInspector()}
        >
          <AlertTriangle size={12} /> Index failed
        </StatusBarItem>
      {:else}
        <StatusBarItem tooltip={bennuIndexStore.typeCount ? `Index ready · ${bennuIndexStore.typeCount} types` : 'Project index is up to date'}>
          <Database size={12} /> Indexed{bennuIndexStore.typeCount ? ` · ${bennuIndexStore.typeCount}` : ''}
        </StatusBarItem>
      {/if}
      <StatusBarSeparator />
    {/if}

    <!-- The language server for the open file, when one owns it.
         This is the Rust counterpart of the index readout above, and it exists for the same
         reason: rust-analyzer needs tens of seconds to become useful on a cold project and
         answers almost nothing until it has. Without a line saying so, "go-to does nothing"
         and "the server is still loading the workspace" look identical. -->
    {#if lsp}
      {#if lsp.state === 'starting' || lsp.progress}
        <!-- A server's progress message is free text — rust-analyzer puts absolute paths in it —
             and the footer is one row. The backend already caps the string; the width cap is the
             second line of defence. -->
        <StatusBarItem
          tone="accent"
          maxWidth={30}
          tooltip={`${lsp.name}${lsp.progress ? ` · ${lsp.progress}` : ' · starting'} — click for language server settings`}
          onclick={openLanguageSettings}
        >
          <Spinner size={11} /> {lsp.progress || `${lsp.name} starting`}…
        </StatusBarItem>
      {:else if lsp.state === 'failed' || lsp.state === 'exited'}
        <StatusBarItem
          tone="warning"
          tooltip={lsp.message || `${lsp.name} is not running — click to fix`}
          onclick={openLanguageSettings}
        >
          <ServerCrash size={12} /> {lsp.name}
        </StatusBarItem>
      {:else}
        <StatusBarItem
          tooltip={`${lsp.version ?? lsp.name} · ${lsp.features.length} features · ${tokens} semantic tokens in this buffer — click for language server settings`}
          onclick={openLanguageSettings}
        >
          <ServerCog size={12} /> {lsp.name}
        </StatusBarItem>
      {/if}
      <StatusBarSeparator />
    {/if}

    <!-- Indentation (tabs/spaces + width) — click / keyboard to change; applies live. -->
    <BennuIndentStatus />

    {#if encoding}
      <StatusBarSeparator />
      <StatusBarItem tooltip={isCargo ? 'Rust source is UTF-8 by language definition' : 'Project source encoding (pom sourceEncoding)'}>
        <FileType size={12} /> {encoding}
      </StatusBarItem>
    {/if}
    <!-- The open file's own encoding + caret Ln/Col live on the editor's footer (BennuEditor). -->
  {/if}

  {#if footerExtra}
    <StatusBarSeparator />
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
  .bf-sub {
    font-size: var(--font-size-2xs); color: var(--text-disabled);
    padding-left: 2px; max-width: 160px; overflow: hidden; text-overflow: ellipsis;
  }
  .bf-spacer { flex: 1; }
</style>
