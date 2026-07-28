<script lang="ts">
  /**
   * Scripts panel — the repository on disk, one sub-tree per dialect branch.
   *
   * This is where the product's structural rule is visible: the dialect belongs
   * to the FOLDER. The same logical change lives twice, in two syntaxes, under
   * two branches — so the tree shows the dialect on the branch and the role on
   * the folder, and every file row carries its encoding and line ending, because
   * a file silently rewritten as UTF-8 is one of the failures Picus exists to
   * catch.
   *
   * The repository shown is **the active connection's**: Picus is database
   * oriented, so you open a database and its scripts are what you get. A
   * connection with none attached is offered a folder to point at, rather than
   * leaving the panel to look broken.
   */
  import { FolderTree, ChevronRight, Folder, FileCode2, RefreshCw, FolderOpen, Database } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import EncodingPill from '$lib/components/shared/internal/EncodingPill.svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import PicusRoleChip from '../PicusRoleChip.svelte';
  import NoticeList from './NoticeList.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import type { ScriptFile } from '$lib/types/picus';

  let query = $state('');
  const needle = $derived(query.trim().toLowerCase());

  const connection = $derived(connectionsStore.active);
  const attached = $derived(picusProjectStore.attached);

  function matches(f: ScriptFile): boolean {
    return !needle || f.name.toLowerCase().includes(needle) || f.path.toLowerCase().includes(needle);
  }

  function openFile(path: string) {
    const file = picusProjectStore.fileByPath(path);
    if (!file) return;
    picusTabsStore.openFile(file.path, file.name, picusProjectStore.dialectOfFile(file.path));
  }

  /** Marker meaning for the coloured dot on a file row. */
  const STATUS_HINT = {
    modified: 'Modified since the last index',
    new: 'Created by a generation and not yet reviewed',
    error: 'Has a blocking finding',
  } as const;
</script>

<PanelShell title="Scripts on disk" count={picusProjectStore.fileCount}>
  {#snippet icon()}<FolderTree size={13} />{/snippet}

  {#snippet actions()}
    <Button
      variant="icon"
      size="xs"
      tooltip={{ content: 'Re-read the repository from disk', shortcut: 'F5' }}
      ariaLabel="Re-read the repository from disk"
      disabled={!attached || picusProjectStore.loading}
      onclick={() => void picusProjectStore.refresh()}
    >
      {#snippet iconStart()}<RefreshCw size={13} />{/snippet}
    </Button>
    <Button
      variant="icon"
      size="xs"
      tooltip={attached
        ? 'Point this connection at another folder of scripts'
        : 'Attach the folder of scripts this database is installed from'}
      ariaLabel="Attach a script repository"
      disabled={!connection}
      onclick={() => connection && picusUiStore.openScriptRootPicker(connection.id)}
    >
      {#snippet iconStart()}<FolderOpen size={13} />{/snippet}
    </Button>
  {/snippet}

  {#snippet toolbar()}
    <SearchBar bind:query showRegex={false} placeholder="Filter files" ariaLabel="Filter files" />
  {/snippet}

  {#if !connection}
    <StateBlock
      tone="info"
      fill={false}
      label="No connection selected. A repository of scripts belongs to the database it installs — pick one under Connections."
    />
  {:else if !attached}
    <div class="sp-attach">
      <StateBlock tone="info" fill={false}>
        <div class="sp-attach-text">
          <strong>{connection.name} has no scripts attached.</strong>
          <span>
            Point it at the folder this database is installed from — the one holding a
            branch per dialect. Picus reads the layout, indexes the objects and checks the
            branches against each other.
          </span>
        </div>
      </StateBlock>
      <Button
        variant="primary"
        size="sm"
        onclick={() => picusUiStore.openScriptRootPicker(connection.id)}
      >
        {#snippet iconStart()}<FolderOpen size={13} />{/snippet}
        Attach a folder…
      </Button>
    </div>
  {:else if picusProjectStore.loading && !picusProjectStore.branches.length}
    <StateBlock tone="loading">
      {#snippet spinner()}<Spinner size={14} />{/snippet}
      <span>Reading {picusProjectStore.root}…</span>
    </StateBlock>
  {:else if picusProjectStore.error}
    <div class="sp-error">
      <Alert variant="error" compact title="This folder could not be read" text={picusProjectStore.error} />
      <div class="sp-error-actions">
        <Button variant="secondary" size="xs" onclick={() => void picusProjectStore.refresh()}>Try again</Button>
        <Button
          variant="ghost"
          size="xs"
          onclick={() => picusUiStore.openScriptRootPicker(connection.id)}
        >
          Choose another folder…
        </Button>
      </div>
    </div>
  {:else if !picusProjectStore.branches.length}
    <StateBlock
      tone="info"
      fill={false}
      label="Nothing that looks like a script branch was found under this folder."
    />
  {:else}
    <!-- The reader's questions come before the tree: a folder it could not
         classify changes what every row below it means. -->
    <NoticeList notes={picusProjectStore.problems} label="Needs an answer" onOpen={openFile} />

    {#if picusProjectStore.isNew}
      <div class="sp-inferred">
        <Alert
          variant="info"
          compact
          text="This layout was inferred from the folder names — nothing has been written into the repository. Branches, roles and encodings below are Picus's reading of it."
        />
      </div>
    {/if}

    {#each picusProjectStore.branches as branch (branch.id)}
      <SidebarItem onclick={() => picusProjectStore.toggle(branch.id)}>
        {#snippet icon()}
          <span class="sp-twist" class:sp-open={picusProjectStore.isExpanded(branch.id)}>
            <ChevronRight size={12} />
          </span>
        {/snippet}
        <span class="sp-branch">{branch.label}</span>
        {#snippet badges()}
          <PicusDialectChip dialect={branch.dialect} />
        {/snippet}
      </SidebarItem>

      {#if picusProjectStore.isExpanded(branch.id)}
        {#each branch.folders as folder (folder.id)}
          {@const files = folder.files.filter(matches)}
          <SidebarItem indent={22} onclick={() => picusProjectStore.toggle(folder.id)}>
            {#snippet icon()}
              <span class="sp-twist" class:sp-open={picusProjectStore.isExpanded(folder.id)}>
                <ChevronRight size={12} />
              </span>
            {/snippet}
            <Folder size={13} class="sp-folder-icon" />
            <span class="sp-folder">{folder.label}</span>
            {#snippet badges()}
              <PicusRoleChip role={folder.role} terse />
              <Badge variant="count" label={String(files.length)} />
            {/snippet}
          </SidebarItem>

          {#if picusProjectStore.isExpanded(folder.id)}
            {#each files as file (file.path)}
              <SidebarItem
                indent={40}
                selected={picusTabsStore.active?.file === file.path}
                onclick={() => picusTabsStore.openFile(file.path, file.name, branch.dialect)}
              >
                {#snippet icon()}<FileCode2 size={13} />{/snippet}
                <span class="sp-file">{file.name}</span>
                {#if file.status}
                  <span class="sp-mark sp-{file.status}" title={STATUS_HINT[file.status]}></span>
                {/if}
                {#snippet badges()}
                  <EncodingPill
                    encoding={file.encoding}
                    expected={file.expectedEncoding}
                    eol={file.eol}
                    compact
                  />
                {/snippet}
              </SidebarItem>
            {/each}
            {#if !files.length}
              <p class="sp-none">No file matches the filter.</p>
            {/if}
          {/if}
        {/each}
      {/if}
    {/each}

    <NoticeList notes={picusProjectStore.notes} label="What Picus inferred" onOpen={openFile} />

    <p class="sp-hint">
      The dialect is a property of the folder: the same data is written in two different
      forms, once per branch.
    </p>
    <p class="sp-root" title={picusProjectStore.root}>
      <Database size={11} />
      {connection.name} · {picusProjectStore.root}
    </p>
  {/if}
</PanelShell>

<style>
  .sp-twist {
    display: inline-flex;
    color: var(--text-disabled);
    transition: transform var(--transition-fast);
  }
  .sp-twist.sp-open { transform: rotate(90deg); }

  .sp-branch {
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-secondary);
  }
  .sp-folder { overflow: hidden; text-overflow: ellipsis; }
  .sp-file {
    font-family: var(--font-code);
    font-size: 11.5px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Working-copy markers: new / modified / has a blocking finding. */
  .sp-mark {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
    margin-left: 4px;
  }
  .sp-modified { background: var(--warning); }
  .sp-new { background: var(--success); }
  .sp-error { background: var(--error); }

  .sp-none,
  .sp-hint {
    padding: 6px 12px 6px 44px;
    font-size: 11px;
    color: var(--text-disabled);
    font-style: italic;
  }
  .sp-hint {
    padding: 10px 12px;
    font-style: normal;
    line-height: 1.5;
    color: var(--text-muted);
  }

  /* Which database's repository this is — the panel's whole framing in one line. */
  .sp-root {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 0 12px 10px;
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-disabled);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sp-attach {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    padding: 4px 12px 12px;
  }
  .sp-attach-text { display: flex; flex-direction: column; gap: 4px; text-align: left; }
  .sp-attach-text strong { font-size: 12px; }
  .sp-attach-text span { font-size: 11.5px; line-height: 1.5; color: var(--text-muted); }

  .sp-error { display: flex; flex-direction: column; gap: 8px; padding: 8px 12px; }
  .sp-error-actions { display: flex; gap: 6px; }

  .sp-inferred { padding: 4px 12px 8px; }

  :global(.sp-folder-icon) { color: var(--text-muted); flex-shrink: 0; }
</style>
