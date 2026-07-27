<script lang="ts">
  /**
   * Add a destination — pick the file this generation should also be written to.
   *
   * The list is the project itself, grouped by branch and folder, so the choice
   * carries its dialect and its role with it: picking a file inside
   * `ORACLE/AGGIORNAMENTO` gives you an Oracle update destination with the
   * update preset already applied, and nothing has to be re-stated.
   *
   * Files already among the destinations are shown as such rather than hidden —
   * "why is it not in the list" is a worse question than "it is already there".
   */
  import { FilePlus2, FileCode2, Check, Search } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import PicusRoleChip from '../PicusRoleChip.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { dmlStore, presetForRole } from '$lib/stores/picus/dml.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import type { Branch, FolderRole, ScriptFolder } from '$lib/types/picus';

  let { onClose }: { onClose: () => void } = $props();

  let query = $state('');
  /** Name for a file that does not exist yet, inside the selected folder. */
  let newFileName = $state('');
  let newFileFolder = $state<{ branch: Branch; folder: ScriptFolder } | null>(null);

  const needle = $derived(query.trim().toLowerCase());
  const existingFiles = $derived(new Set(dmlStore.targets.map((t) => t.file)));

  function matches(path: string, name: string) {
    return !needle || path.toLowerCase().includes(needle) || name.toLowerCase().includes(needle);
  }

  /** What the preset for a role will do — stated before the choice, not after. */
  function presetSummary(role: FolderRole): string {
    const p = presetForRole(role);
    if (p.wrap === 'plain') return 'bare statements, no guards';
    const bits = ['procedural block'];
    if (p.guards.version) bits.push('version guard');
    if (p.guards.skipIfPresent) bits.push('skip existing');
    return bits.join(' · ');
  }

  function add(branch: Branch, folder: ScriptFolder, file: string) {
    dmlStore.addTarget({ file, dialect: branch.dialect, role: folder.role, branchId: branch.id });
    toastStore.show(`${file} added as a destination.`, 'success');
    onClose();
  }

  function addNew() {
    if (!newFileFolder || !newFileName.trim()) return;
    const { branch, folder } = newFileFolder;
    const name = newFileName.trim().endsWith('.sql') ? newFileName.trim() : `${newFileName.trim()}.sql`;
    add(branch, folder, `${folder.path}/${name}`);
  }
</script>

<Modal {onClose} width="680px" height="580px" padBody={false} ariaLabel="Add a destination">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FilePlus2 size={14} />
      <span class="modal-title">Add a destination</span>
    </ModalHeader>
  {/snippet}

  <div class="ad">
    <div class="ad-toolbar">
      <SearchBar bind:query showRegex={false} showCounter={false} placeholder="Filter files…" ariaLabel="Filter files" autofocus />
    </div>

    <div class="ad-list">
      {#if !picusProjectStore.branches.length}
        <StateBlock tone="info" fill={false} label="No project open — there is nowhere to write." />
      {/if}

      {#each picusProjectStore.branches as branch (branch.id)}
        {#each branch.folders as folder (folder.id)}
          {@const files = folder.files.filter((f) => matches(f.path, f.name))}
          {#if files.length || !needle}
            <div class="ad-group">
              <span class="ad-group-name">{branch.label} / {folder.label}</span>
              <PicusDialectChip dialect={branch.dialect} terse />
              <PicusRoleChip role={folder.role} terse />
              <span class="ad-spacer"></span>
              <span class="ad-preset">{presetSummary(folder.role)}</span>
            </div>

            {#each files as file (file.path)}
              {@const already = existingFiles.has(file.path)}
              <button
                class="ad-row"
                class:ad-already={already}
                disabled={already}
                onclick={() => add(branch, folder, file.path)}
              >
                <FileCode2 size={13} />
                <span class="ad-name">{file.name}</span>
                <span class="ad-path">{file.path}</span>
                <span class="ad-spacer"></span>
                {#if already}
                  <Badge variant="tone" tone="neutral" size="sm" label="already a destination" />
                {:else}
                  <Check size={13} class="ad-tick" />
                {/if}
              </button>
            {/each}

            {#if !needle}
              <!-- A new file inside this folder: generation often introduces the
                   next update script rather than appending to an existing one. -->
              <div class="ad-new" class:ad-new-open={newFileFolder?.folder.id === folder.id}>
                {#if newFileFolder?.folder.id === folder.id}
                  <Input
                    value={newFileName}
                    size="sm"
                    placeholder="4_13__4_14.sql"
                    ariaLabel="New file name"
                    oninput={(v) => (newFileName = v)}
                  />
                  <Button variant="primary" size="xs" disabled={!newFileName.trim()} onclick={addNew}>Create</Button>
                  <Button variant="ghost" size="xs" onclick={() => (newFileFolder = null)}>Cancel</Button>
                {:else}
                  <Button
                    variant="ghost"
                    size="xs"
                    onclick={() => { newFileFolder = { branch, folder }; newFileName = ''; }}
                  >
                    {#snippet iconStart()}<FilePlus2 size={12} />{/snippet}
                    New file in this folder…
                  </Button>
                {/if}
              </div>
            {/if}
          {/if}
        {/each}
      {/each}

      {#if needle && !picusProjectStore.allFiles.some((f) => matches(f.path, f.name))}
        <div class="ad-empty">
          <Search size={22} strokeWidth={1.5} />
          <p>No file matches “{query.trim()}”.</p>
        </div>
      {/if}
    </div>
  </div>

  {#snippet footer()}
    <span class="ad-foot">
      A destination inherits the dialect of its branch and the preset of its folder's role;
      both stay editable afterwards.
    </span>
    <Button variant="ghost" size="sm" onclick={onClose}>Close</Button>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }

  .ad { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .ad-toolbar { flex-shrink: 0; padding: 12px 16px; border-bottom: 1px solid var(--border-subtle); }
  .ad-list { flex: 1; min-height: 0; overflow-y: auto; padding: 8px 0 16px; }

  .ad-group {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 9px 16px 5px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .ad-group-name { white-space: nowrap; }
  .ad-preset {
    font-family: var(--font-code);
    font-size: 10px;
    font-weight: 400;
    letter-spacing: 0;
    text-transform: none;
    color: var(--text-disabled);
  }
  .ad-spacer { flex: 1; }

  .ad-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 16px;
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .ad-row:hover:not(:disabled) { background: var(--bg-hover); }
  .ad-row:disabled { cursor: default; opacity: 0.55; }
  .ad-row :global(svg) { color: var(--text-disabled); flex-shrink: 0; }
  .ad-row:hover:not(:disabled) :global(.ad-tick) { color: var(--success); }
  .ad-row :global(.ad-tick) { color: transparent; }

  .ad-name { font-family: var(--font-code); font-size: 11.5px; white-space: nowrap; }
  .ad-path {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-disabled);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ad-new { display: flex; align-items: center; gap: 6px; padding: 2px 16px 6px 34px; }
  .ad-new-open { padding-top: 6px; }

  .ad-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 48px 16px;
    color: var(--text-disabled);
  }
  .ad-empty p { margin: 0; font-size: 12px; }

  .ad-foot {
    flex: 1;
    font-size: 11px;
    line-height: 1.45;
    color: var(--text-muted);
    text-align: left;
  }
</style>
