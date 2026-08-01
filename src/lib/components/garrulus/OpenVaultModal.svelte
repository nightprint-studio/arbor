<script lang="ts">
  /**
   * Open, switch or create a vault — the door to the whole product.
   *
   * A vault is a folder of markdown notes, so "open" is genuinely "point at a
   * folder". Two ways in, on two pages of one dialog because they are the same
   * decision made twice: **Open** lists the vaults this profile already knows
   * (`garrulus/vaults.json`, most recently opened first) and can also browse to
   * one that is not on the list; **Create** turns an empty folder into a vault —
   * the `.arbor/garrulus/` marker, the default settings and the built-in note
   * types — and opens it.
   *
   * Keyboard-first, like every picker in Arbor: the filter box has focus on open,
   * ↑/↓ walk the list without leaving it, Enter opens the highlighted vault,
   * Ctrl+Enter is the page's primary action and Esc cancels. The folder picker is
   * Arbor's own explorer in folder mode — never the native dialog.
   */
  import { onMount, untrack } from 'svelte';
  import { FolderOpen, FolderPlus, Library, Search } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { fuzzyMatchPair } from '$lib/utils/fuzzy';
  import { vaultLastSeen } from '$lib/stores/garrulus/vault.svelte';
  import {
    createVault,
    listVaults,
    openVault,
    type VaultEntry,
    type VaultSummary,
  } from '$lib/ipc/garrulus';

  type Page = 'open' | 'create';

  interface Props {
    onClose: () => void;
    /** A vault is open. The host titles the window after it and loads the tree. */
    onOpened?: (vault: VaultSummary) => void;
    /** Root of the vault already open, so the list can say which one that is. */
    currentRoot?: string | null;
    /** Which half to land on. "Create a vault" is its own verb in the palette,
     *  and landing it on the list it is not looking for is a wasted click. */
    initialPage?: Page;
  }

  let { onClose, onOpened, currentRoot = null, initialPage = 'open' }: Props = $props();

  // The starting page only, not a binding: the tabs below own it from here.
  let page = $state<Page>(untrack(() => initialPage));

  const pages: TabItem[] = [
    { id: 'open', label: 'Open' },
    { id: 'create', label: 'Create' },
  ];

  // ── The known vaults ────────────────────────────────────────────────────────
  let entries = $state<VaultEntry[]>([]);
  let loaded = $state(false);
  let error = $state<string | null>(null);
  let busy = $state(false);

  // `onMount` rather than a dependency-less `$effect`: this is a one-shot load,
  // and an effect that does IO is one careless `$state` read away from a loop.
  onMount(() => {
    void listVaults()
      .then((list) => { entries = list; })
      // A registry that cannot be read is not an empty registry: the browse
      // button below still works, and the message says which of the two it is.
      .catch((e) => { error = String(e); })
      .finally(() => { loaded = true; });
  });

  let query = $state('');
  let selectedIdx = $state(0);
  let listEl = $state<HTMLElement | undefined>();
  let filterEl = $state<HTMLInputElement | undefined>();

  // Focus follows the page: the filter on Open, the folder field on Create. An
  // `$effect` rather than `autofocus`, per the a11y rule.
  let folderEl = $state<HTMLInputElement | undefined>();
  $effect(() => {
    if (page === 'open') filterEl?.focus();
    else folderEl?.focus();
  });

  const filtered = $derived.by(() => {
    const q = query.trim();
    if (!q) return entries;
    return entries
      .map((e) => ({ e, m: fuzzyMatchPair(e.display_name, e.path, q) }))
      .filter((r) => r.m !== null)
      .sort((a, b) => (b.m?.score ?? 0) - (a.m?.score ?? 0))
      .map((r) => r.e);
  });

  // Keep the cursor valid as the filter changes the list under it. ONE effect:
  // two of them were doing the same job in two shapes, and the clamp read the
  // same `selectedIdx` it wrote — which converges today only because reassigning
  // 0 over 0 is swallowed by the equality check, and stops converging the moment
  // the clamp is not idempotent. `untrack` around the write keeps the effect
  // keyed on the list, never on the cursor.
  let lastQuery = '';
  $effect(() => {
    const n = filtered.length;
    const q = query;
    untrack(() => {
      if (q !== lastQuery) {
        lastQuery = q;
        selectedIdx = 0;
      } else if (selectedIdx >= n) {
        selectedIdx = Math.max(0, n - 1);
      }
    });
  });

  function scrollSelectionIntoView() {
    requestAnimationFrame(() => {
      listEl?.querySelector<HTMLElement>(`[data-idx="${selectedIdx}"]`)
        ?.scrollIntoView({ block: 'nearest' });
    });
  }

  // ── Opening ─────────────────────────────────────────────────────────────────
  async function open(path: string) {
    if (busy) return;
    busy = true;
    error = null;
    try {
      const summary = await openVault(path);
      onOpened?.(summary);
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function openSelected() {
    const entry = filtered[selectedIdx];
    if (entry) void open(entry.path);
  }

  // ── Creating ────────────────────────────────────────────────────────────────
  let newFolder = $state('');
  let newName = $state('');
  /** Set when a create failed — the folder may simply already be a vault, and
   *  opening it is then the obvious next move rather than a retype. */
  let createFailed = $state(false);

  const canCreate = $derived(newFolder.trim() !== '');

  async function create() {
    if (!canCreate || busy) return;
    busy = true;
    error = null;
    createFailed = false;
    try {
      const summary = await createVault(newFolder.trim(), newName.trim() || undefined);
      onOpened?.(summary);
      onClose();
    } catch (e) {
      error = String(e);
      createFailed = true;
    } finally {
      busy = false;
    }
  }

  // ── The folder picker ───────────────────────────────────────────────────────
  /** Which page asked for a folder — the picker is one component, the two pages
   *  do different things with what it returns. */
  let picking = $state<Page | null>(null);

  function onPicked(path: string) {
    const forPage = picking;
    picking = null;
    if (forPage === 'create') {
      newFolder = path;
      // The folder's own name is what the user already calls it; typing it again
      // is the sort of question a dialog should answer for itself.
      if (!newName.trim()) newName = path.split(/[/\\]/).filter(Boolean).pop() ?? '';
    } else {
      void open(path);
    }
  }

  function onListKeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedIdx = Math.min(selectedIdx + 1, filtered.length - 1);
      scrollSelectionIntoView();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedIdx = Math.max(selectedIdx - 1, 0);
      scrollSelectionIntoView();
    } else if (e.key === 'Enter' && !(e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      openSelected();
    }
    // Esc bubbles to <Modal>, which closes the topmost dialog.
  }

  function onKeyDown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      if (page === 'open') openSelected();
      else void create();
    }
  }
</script>

<Modal {onClose} width="640px" height="560px" padBody={false} ariaLabel="Vaults">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Library size={14} />
      <span class="modal-title">Vaults</span>
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="ov" role="group" aria-label="Vault" onkeydown={onKeyDown}>
    <div class="ov-tabs">
      <Tabs
        items={pages}
        value={page}
        variant="underline"
        size="sm"
        ariaLabel="Open or create a vault"
        onSelect={(id) => { page = id as Page; error = null; }}
      />
    </div>

    {#if page === 'open'}
      <div class="ov-search">
        <Input
          bind:value={query}
          bind:element={filterEl}
          placeholder="Filter vaults…"
          ariaLabel="Filter vaults"
          onkeydown={onListKeydown}
        >
          {#snippet iconStart()}<Search size={14} />{/snippet}
        </Input>
      </div>

      {#if !loaded}
        <StateBlock tone="loading">
          {#snippet spinner()}<Spinner size={14} />{/snippet}
          <span>Reading the vault list…</span>
        </StateBlock>
      {:else if entries.length === 0}
        <EmptyState
          message="No vault yet."
          description="Browse to a folder of markdown notes to open it, or create one on the Create page."
        />
      {:else if filtered.length === 0}
        <EmptyState message="No vault matches that." />
      {:else}
        <div
          class="ov-list"
          bind:this={listEl}
          role="listbox"
          tabindex="-1"
          aria-label="Known vaults"
        >
          {#each filtered as entry, idx (entry.id)}
            {@const when = vaultLastSeen(entry)}
            <button
              type="button"
              class="ov-item"
              class:selected={idx === selectedIdx}
              data-idx={idx}
              role="option"
              aria-selected={idx === selectedIdx}
              onmouseenter={() => (selectedIdx = idx)}
              onclick={() => void open(entry.path)}
            >
              <span class="ov-item-icon"><Library size={15} /></span>
              <span class="ov-item-text">
                <span class="ov-item-name">
                  {entry.display_name}
                  {#if currentRoot && entry.path === currentRoot}
                    <Badge variant="tone" tone="success" size="sm" label="open" />
                  {/if}
                  {#if entry.remote}
                    <Badge variant="tone" tone="neutral" size="sm" label={entry.remote.kind} />
                  {/if}
                </span>
                <span class="ov-item-path">{entry.path}</span>
              </span>
              {#if when}
                <span class="ov-item-when">{when}</span>
              {/if}
            </button>
          {/each}
        </div>
      {/if}
    {:else}
      <div class="ov-create">
        <FormField
          label="Folder"
          required
          hint="Where the notes will live. An existing folder is fine — a vault is a folder of markdown files, and Garrulus adds only its own .arbor/garrulus/ directory to it."
        >
          <div class="ov-row">
            <Input
              bind:value={newFolder}
              bind:element={folderEl}
              placeholder="/home/you/notes"
              ariaLabel="Vault folder"
            />
            <Button
              variant="secondary"
              size="sm"
              ariaLabel="Choose the vault folder"
              onclick={() => (picking = 'create')}
            >
              {#snippet iconStart()}<FolderOpen size={13} />{/snippet}
              Choose…
            </Button>
          </div>
        </FormField>

        <FormField
          label="Name"
          optionalText="(optional)"
          hint="What the switcher calls it. Defaults to the folder's own name."
        >
          <Input bind:value={newName} placeholder="Notes" ariaLabel="Vault name" />
        </FormField>

        <Alert variant="info" compact>
          Creating a vault writes the marker folder, the default settings and the
          built-in note types — nothing else. The types live inside the vault, so
          they travel with it to the other machine.
        </Alert>
      </div>
    {/if}

    {#if error}
      <div class="ov-error">
        <Alert variant="error" title="That did not work" text={error}>
          {#snippet actions()}
            {#if createFailed && newFolder.trim()}
              <Button variant="secondary" size="xs" onclick={() => void open(newFolder.trim())}>
                Open it instead
              </Button>
            {/if}
          {/snippet}
        </Alert>
      </div>
    {/if}
  </div>

  {#snippet footer()}
    {#if page === 'open'}
      <!-- The way in for a vault the registry has never seen. On the Create page
           the folder field already asks the same question. -->
      <Button variant="ghost" size="sm" onclick={() => (picking = 'open')}>
        {#snippet iconStart()}<FolderOpen size={13} />{/snippet}
        Open a folder…
      </Button>
    {/if}
    <span class="ov-spacer"></span>
    <Button variant="ghost" size="sm" onclick={onClose}>Cancel</Button>
    {#if page === 'open'}
      <Button
        variant="primary"
        size="sm"
        loading={busy}
        disabled={filtered.length === 0}
        tooltip={{ content: 'Open the highlighted vault', shortcut: 'Ctrl+Enter' }}
        onclick={openSelected}
      >
        Open
      </Button>
    {:else}
      <Button
        variant="primary"
        size="sm"
        loading={busy}
        disabled={!canCreate}
        tooltip={{ content: 'Create the vault and open it', shortcut: 'Ctrl+Enter' }}
        onclick={() => void create()}
      >
        {#snippet iconStart()}<FolderPlus size={13} />{/snippet}
        Create vault
      </Button>
    {/if}
  {/snippet}
</Modal>

{#if picking}
  <!-- Arbor's own explorer in folder mode. Stacked over this dialog rather than
       replacing it: a half-filled Create page has to survive the choice. -->
  <FileExplorerModal
    mode="folder"
    title={picking === 'create' ? 'Choose the folder for the new vault' : 'Choose the vault folder'}
    initialPath={picking === 'create' ? newFolder || undefined : undefined}
    onConfirm={onPicked}
    onCancel={() => (picking = null)}
    onClose={() => (picking = null)}
  />
{/if}

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }

  .ov {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .ov-tabs {
    padding: 0 12px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  .ov-search { padding: 12px 12px 8px; flex-shrink: 0; }

  .ov-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 6px 8px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .ov-item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 7px 10px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
  }
  .ov-item:hover { background: var(--bg-hover); }
  .ov-item.selected { background: color-mix(in srgb, var(--accent) 10%, transparent); }

  .ov-item-icon { display: flex; color: var(--text-muted); flex-shrink: 0; }
  .ov-item.selected .ov-item-icon { color: var(--accent); }

  .ov-item-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .ov-item-name {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--font-size-sm);
    font-weight: 500;
    min-width: 0;
  }
  .ov-item-path {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }
  .ov-item-when {
    flex-shrink: 0;
    font-size: var(--font-size-xs);
    color: var(--text-disabled);
  }

  .ov-create {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 14px 12px;
  }

  /* Typed path first, picker second: the field stays the source of truth. */
  .ov-row { display: flex; align-items: center; gap: 8px; }
  .ov-row > :global(:first-child) { flex: 1; min-width: 0; }

  .ov-error { padding: 0 12px 12px; flex-shrink: 0; }

  .ov-spacer { flex: 1; }
</style>
