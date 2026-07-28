<script lang="ts">
  /**
   * Add a destination — pick the file this generation should also be written to.
   *
   * The list is the repository itself, one group per folder that holds scripts,
   * so the choice carries its engine and its role with it: picking a file inside
   * `AGGIORNAMENTO/4.13.2/ORA` gives you an Oracle update destination with the
   * update preset already applied, and nothing has to be re-stated. The engine
   * and the role are the folder's **effective** ones — inherited from wherever
   * they were declared, which is the whole point of the inheritance rule.
   *
   * A folder with no engine cannot become a destination: there is no form to
   * write the statements in. Rather than hide it, the group says so and offers
   * the one action that fixes it.
   *
   * Files already among the destinations are shown as such rather than hidden —
   * "why is it not in the list" is a worse question than "it is already there".
   */
  import { FilePlus2, FileCode2, Check, FolderCog, Search } from 'lucide-svelte';
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
  import { picusProjectStore, type FolderEntry } from '$lib/stores/picus/project.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import {
    declaredEngine,
    engineIsUnsupported,
    folderAcceptsGeneration,
    folderEngine,
    isDialect,
    isGenericEngine,
    type FolderRole,
  } from '$lib/types/picus';

  let { onClose }: { onClose: () => void } = $props();

  let query = $state('');
  /** Name for a file that does not exist yet, inside the selected folder. */
  let newFileName = $state('');
  let newFileFolder = $state<FolderEntry | null>(null);

  const needle = $derived(query.trim().toLowerCase());
  const existingFiles = $derived(new Set(dmlStore.targets.map((t) => t.file)));

  /** Only folders that hold scripts can be written into — the rest are structure. */
  const groups = $derived(picusProjectStore.entries.filter((e) => e.node.files.length > 0));

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

  function add(entry: FolderEntry, file: string) {
    const engine = folderEngine(entry.node);
    // A portable folder is a destination like any other; the backend restricts
    // what may be written into it to the intersection of the two dialects, which
    // is the whole payoff — one file instead of two for a plain INSERT.
    if (!isDialect(engine) && !isGenericEngine(engine)) {
      // Should be unreachable — the rows are disabled — but a destination with no
      // engine would silently emit nothing, so it is refused rather than trusted.
      toastStore.show(`${entry.node.path} has no engine. Say which database it is for first.`, 'warning');
      return;
    }
    dmlStore.addTarget({ file, dialect: engine, role: entry.node.effectiveRole });
    toastStore.show(`${file} added as a destination.`, 'success');
    onClose();
  }

  function addNew() {
    if (!newFileFolder || !newFileName.trim()) return;
    const name = newFileName.trim().endsWith('.sql') ? newFileName.trim() : `${newFileName.trim()}.sql`;
    add(newFileFolder, `${newFileFolder.node.path}/${name}`);
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
      {#if !groups.length}
        <StateBlock tone="info" fill={false} label="No repository open — there is nowhere to write." />
      {/if}

      {#each groups as entry (entry.node.path)}
        {@const folder = entry.node}
        {@const files = folder.files.filter((f) => matches(f.path, f.name))}
        {@const writable = folderAcceptsGeneration(folder)}
        {#if files.length || !needle}
          <div class="ad-group">
            <span class="ad-group-name" title={folder.path}>{folder.path}</span>
            <PicusDialectChip
              engine={folderEngine(folder)}
              terse
              inherited={declaredEngine(folder) === null}
              from={entry.dialectFrom ?? ''}
            />
            <PicusRoleChip
              role={folder.effectiveRole}
              terse
              inherited={folder.role === null}
              from={entry.roleFrom ?? ''}
            />
            <span class="ad-spacer"></span>
            {#if writable}
              <span class="ad-preset">{presetSummary(folder.effectiveRole)}</span>
            {:else if engineIsUnsupported(folder)}
              <!-- An engine Picus does not read is not an unanswered question, so
                   it gets a statement rather than a call to action: there is
                   nothing here for the user to fix. -->
              <span class="ad-preset">Picus does not generate this engine</span>
            {:else}
              <!-- Not a disabled group with no explanation: the reason, and the
                   one action that removes it. -->
              <Button
                variant="ghost"
                size="xs"
                onclick={() => picusUiStore.openFolderClassify(folder.path)}
              >
                {#snippet iconStart()}<FolderCog size={12} />{/snippet}
                No engine — say which…
              </Button>
            {/if}
          </div>

          {#each files as file (file.path)}
            {@const already = existingFiles.has(file.path)}
            <button
              class="ad-row"
              class:ad-already={already || !writable}
              disabled={already || !writable}
              onclick={() => add(entry, file.path)}
            >
              <FileCode2 size={13} />
              <span class="ad-name">{file.name}</span>
              <span class="ad-path">{file.path}</span>
              <span class="ad-spacer"></span>
              {#if already}
                <Badge variant="tone" tone="neutral" size="sm" label="already a destination" />
              {:else if writable}
                <Check size={13} class="ad-tick" />
              {/if}
            </button>
          {/each}

          {#if !needle && writable}
            <!-- A new file inside this folder: generation often introduces the
                 next update script rather than appending to an existing one. -->
            <div class="ad-new" class:ad-new-open={newFileFolder?.node.path === folder.path}>
              {#if newFileFolder?.node.path === folder.path}
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
                  onclick={() => { newFileFolder = entry; newFileName = ''; }}
                >
                  {#snippet iconStart()}<FilePlus2 size={12} />{/snippet}
                  New file in this folder…
                </Button>
              {/if}
            </div>
          {/if}
        {/if}
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
      A destination takes the engine and the role its folder effectively has — inherited or
      declared, it makes no difference here — and the preset that role implies. All of it
      stays editable afterwards.
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
  /* The group IS a path, so it reads as one: code font, real case, ellipsised
     from the left of the row rather than wrapping the header onto two lines. */
  .ad-group-name {
    font-family: var(--font-code);
    font-size: 10.5px;
    font-weight: 500;
    letter-spacing: 0;
    text-transform: none;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 60%;
  }
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
