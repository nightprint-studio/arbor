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
   */
  import { FolderTree, ChevronRight, Folder, FileCode2, RefreshCw, FolderOpen } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import EncodingPill from '$lib/components/shared/internal/EncodingPill.svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import PicusRoleChip from '../PicusRoleChip.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import type { ScriptFile } from '$lib/types/picus';

  let query = $state('');
  const needle = $derived(query.trim().toLowerCase());

  function matches(f: ScriptFile): boolean {
    return !needle || f.name.toLowerCase().includes(needle) || f.path.toLowerCase().includes(needle);
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
      title="Re-scan the project"
      ariaLabel="Re-scan the project"
      onclick={() => toastStore.show('Project re-scanned.', 'success')}
    >
      {#snippet iconStart()}<RefreshCw size={13} />{/snippet}
    </Button>
    <Button
      variant="icon"
      size="xs"
      tooltip={{ content: 'Open a script project', shortcut: 'Ctrl+O' }}
      ariaLabel="Open a script project"
      onclick={() => toastStore.show('Project opening lands with the filesystem milestone.', 'info')}
    >
      {#snippet iconStart()}<FolderOpen size={13} />{/snippet}
    </Button>
  {/snippet}

  {#snippet toolbar()}
    <SearchBar bind:query showRegex={false} placeholder="Filter files" ariaLabel="Filter files" />
  {/snippet}

  {#if !picusProjectStore.branches.length}
    <StateBlock tone="info" fill={false} label="No project open. Open a folder of SQL scripts to index it." />
  {:else}
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

    <p class="sp-hint">
      The dialect is a property of the folder: the same data is written in two different
      forms, once per branch.
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

  :global(.sp-folder-icon) { color: var(--text-muted); flex-shrink: 0; }
</style>
