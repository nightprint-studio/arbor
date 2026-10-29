<script lang="ts">
  /**
   * "Add component" / "Insert resource": pick a registered type, then edit the value it starts with.
   *
   * Two steps in one dialog because the common case is a keyboard one: type a few letters of the
   * name, `Enter` to take the match, edit the placeholder, `Ctrl+Enter` to insert. The placeholder is
   * a well-formed skeleton built from the game's own registry, so the user edits a valid document
   * instead of writing one from nothing.
   */
  import { untrack } from 'svelte';
  import { Plus } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import Tree from '$lib/components/shared/ui/Tree.svelte';
  import { brpTypeShape, brpTypes, type BrpShapeInfo, type BrpTypeInfo } from '$lib/ipc/bennu/brp';
  import { bennuBrpStore } from '$lib/stores/bennu/brp.svelte';
  import BrpJsonEditor from './BrpJsonEditor.svelte';
  import { shortTypeName } from './brp-value';

  let {
    target,
    initial,
    onClose,
  }: {
    /** A component goes onto the selected entity; a resource goes into the world. */
    target: 'component' | 'resource';
    /** Skip the picker: go straight to editing this type. */
    initial?: BrpTypeInfo;
    onClose: () => void;
  } = $props();

  let query = $state('');
  let results = $state<BrpTypeInfo[]>([]);
  let cursor = $state<string | null>(null);
  let chosen = $state<BrpTypeInfo | null>(null);
  let chosenShape = $state<BrpShapeInfo | null>(null);
  let loadingShape = $state(false);

  const noun = $derived(target === 'component' ? 'component' : 'resource');
  const nodes = $derived(results.map((t) => ({ id: t.type_path, info: t })));

  let token = 0;
  $effect(() => {
    const q = query;
    const mine = ++token;
    void brpTypes({ kind: target, query: q, limit: 200 }).then((r) => {
      if (mine !== token) return;
      results = r.items;
      cursor = r.items[0]?.type_path ?? null;
    }).catch(() => { results = []; });
  });

  // Opened from the type browser with the type already chosen.
  $effect(() => { if (initial) untrack(() => void choose(initial)); });

  function move(delta: number) {
    if (results.length === 0) return;
    const i = Math.max(0, results.findIndex((t) => t.type_path === cursor));
    cursor = results[Math.min(results.length - 1, Math.max(0, i + delta))].type_path;
  }

  async function choose(info: BrpTypeInfo | undefined) {
    if (!info) return;
    chosen = info;
    loadingShape = true;
    try {
      chosenShape = await brpTypeShape(info.type_path);
    } catch {
      chosenShape = null;
    } finally {
      loadingShape = false;
    }
  }

  function onSearchKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') { e.preventDefault(); move(1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); move(-1); }
    else if (e.key === 'Enter') {
      e.preventDefault();
      void choose(results.find((t) => t.type_path === cursor));
    }
  }

  async function insert(value: unknown): Promise<boolean> {
    if (!chosen) return false;
    return target === 'component'
      ? bennuBrpStore.insertComponent(chosen.type_path, value)
      : bennuBrpStore.replaceResource(chosen.type_path, value);
  }
</script>

<Modal {onClose} width="560px" height="520px" padBody={false} ariaLabel={`Add ${noun}`}>
  {#snippet header()}
    <ModalHeader {onClose}>
      <Plus size={14} />
      <span class="modal-title">
        {chosen ? `${target === 'component' ? 'Add' : 'Insert'} ${shortTypeName(chosen.type_path)}` : `Add ${noun}`}
      </span>
    </ModalHeader>
  {/snippet}

  {#if chosen}
    <div class="am-edit">
      <p class="am-path">{chosen.type_path}</p>
      {#if loadingShape}
        <EmptyState compact message="Reading the type…" />
      {:else}
        <BrpJsonEditor
          value={chosenShape?.skeleton ?? null}
          applyLabel={target === 'component' ? 'Add' : 'Insert'}
          onApply={insert}
          onCancel={() => { chosen = null; }}
          onDone={onClose}
        />
      {/if}
    </div>
  {:else}
    <div class="am-pick">
      <div class="am-search">
        <SearchBar
          bind:query
          showRegex={false}
          showCounter={false}
          autofocus
          placeholder={`Search registered ${noun}s…`}
          ariaLabel={`Search ${noun}s`}
          onkeydown={onSearchKey}
        />
      </div>
      <div class="am-list">
        {#if nodes.length === 0}
          <EmptyState message={`No registered ${noun} matches.`} description="Only types the game registered with the type registry can be added." />
        {:else}
          <Tree
            {nodes}
            selectedId={cursor}
            getChildren={() => null}
            ariaLabel={`Registered ${noun}s`}
            onSelect={(n) => { cursor = n.id; }}
            onActivate={(n) => void choose(n.info)}
          >
            {#snippet row({ node })}
              <span class="am-name">{node.info.short_path}</span>
              {#if node.info.crate_name}<Badge variant="chip" size="sm" label={node.info.crate_name} />{/if}
              <span class="am-full">{node.info.type_path}</span>
            {/snippet}
          </Tree>
        {/if}
      </div>
    </div>
  {/if}
</Modal>

<style>
  .am-pick, .am-edit { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .am-search { padding: 8px 10px; flex: none; }
  .am-list { flex: 1; min-height: 0; overflow-y: auto; }
  .am-edit { padding: 10px; gap: 8px; overflow-y: auto; }
  .am-path { margin: 0; font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-muted); word-break: break-all; }
  .am-name { font-size: var(--font-size-sm); color: var(--text-primary); }
  .am-full { font-size: var(--font-size-2xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
</style>
