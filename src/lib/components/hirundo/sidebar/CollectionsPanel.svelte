<script lang="ts">
  /**
   * CollectionsPanel — the project's collections in the shared `Tree`. A collection or folder reads as a
   * container (folder glyph that opens, heavier name, how many requests it holds, the auth it sets); a
   * request reads as a request (its method, then its name). Single click = preview tab, Enter or double
   * click = a tab that stays, F2 renames, Del deletes, and a right click raises the same verbs as a menu.
   * Typing in the filter narrows the tree loosely (letters in order, anywhere).
   */
  import { Download, Folder, FolderOpen, FilePlus2, FolderPlus, Key, Library, Trash2 } from 'lucide-svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FilterInput from '$lib/components/shared/ui/FilterInput.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Tree, { type RowSnippetCtx } from '$lib/components/shared/ui/Tree.svelte';
  import { fuzzyMatch } from '$lib/utils/fuzzy';
  import { hirundoContextMenuStore } from '$lib/stores/hirundo/contextmenu.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import { hirundoCommands } from '../hirundo-commands';
  import { methodColor, methodShort, toTreeRows, type TreeRow } from '../request-model';
  import { menuFor, onRowKey, perform } from './collection-menu';

  let query = $state('');
  let contextId = $state<string | null>(null);

  const rows = $derived(toTreeRows(hirundoStore.tree));
  const activePath = $derived(hirundoStore.active?.path ?? null);

  const importItems: DropdownItem[] = [
    { kind: 'item', id: 'curl', label: 'cURL command…', shortcut: 'Ctrl+Shift+U', onclick: hirundoCommands.curl },
    { kind: 'item', id: 'http', label: '.http file (IntelliJ)…', onclick: hirundoCommands.importHttp },
    { kind: 'item', id: 'openapi', label: 'OpenAPI spec (JSON)…', shortcut: 'Ctrl+Shift+Y', onclick: hirundoCommands.importOpenApi },
    { kind: 'item', id: 'soap', label: 'WSDL or SOAP envelope…', onclick: hirundoCommands.importSoap },
  ];

  const countOf = (r: TreeRow): number => r.children.reduce((n, c) => n + (c.kind === 'request' ? 1 : countOf(c)), 0);

  function onContext(row: TreeRow, e: MouseEvent) {
    e.preventDefault();
    contextId = row.id;
    hirundoContextMenuStore.show(e.clientX, e.clientY, menuFor(row), (id) => {
      contextId = null;
      perform(id, row);
    });
  }

  function onKeys(row: TreeRow, e: KeyboardEvent) {
    if (onRowKey(row, e)) e.preventDefault();
  }
</script>

<div class="cp">
  {#if !hirundoStore.project}
    <EmptyState message="No project open" description="Collections live in the .hirundo folder of a project." />
    <div class="cp-cta"><Button variant="primary" onclick={hirundoCommands.openProject}>Open project…</Button></div>
  {:else}
    <div class="cp-top">
      <FilterInput bind:value={query} placeholder="Filter requests…" ariaLabel="Filter requests" />
      <IconButton tooltip="New collection" shortcut="Ctrl+Shift+N" size={26} bordered onclick={hirundoCommands.newCollection}><Library size={14} /></IconButton>
      <Dropdown items={importItems} width="260px">
        {#snippet trigger({ open, toggle })}
          <IconButton tooltip="Import…" size={26} bordered active={open} ariaHasPopup ariaExpanded={open} onclick={toggle}><Download size={14} /></IconButton>
        {/snippet}
      </Dropdown>
      <IconButton tooltip="New SOAP request" size={26} bordered onclick={hirundoCommands.newSoapRequest}><FilePlus2 size={14} /></IconButton>
    </div>
    <div class="cp-tree">
      <Tree
        nodes={rows}
        selectedId={activePath}
        {contextId}
        filter={query}
        match={(n, q) => fuzzyMatch(n.name, q) !== null}
        hasChildren={(n) => n.kind !== 'request'}
        initialExpanded={() => true}
        indentSize={16}
        basePadding={8}
        rowHeight={26}
        guides
        ariaLabel="Collections"
        onSelect={(n) => { if (n.kind === 'request') void hirundoStore.openRequest(n.id, true); }}
        onActivate={(n) => { if (n.kind === 'request') void hirundoStore.openRequest(n.id, false); }}
        onContextMenu={onContext}
        onRowKeydown={onKeys}
        rowTitle={(n) => n.error ?? undefined}
      >
        {#snippet row({ node, expanded }: RowSnippetCtx<TreeRow>)}
          {#if node.kind === 'request'}
            <span class="cp-method" style:color={methodColor(node.method ?? 'GET')}>{methodShort(node.method ?? 'GET')}</span>
            <span class="cp-name" class:cp-bad={!!node.error}>{node.name}</span>
            <span class="cp-spacer"></span>
            <span class="cp-acts">
              <IconButton tooltip="Delete" shortcut="Del" size={20} variant="danger" onclick={(e) => { e.stopPropagation(); perform('delete', node); }}><Trash2 size={12} /></IconButton>
            </span>
          {:else}
            <span class="tree-icon cp-folder" class:cp-collection={node.kind === 'collection'}>
              {#if node.kind === 'collection'}<Library size={14} />{:else if expanded}<FolderOpen size={14} />{:else}<Folder size={14} />{/if}
            </span>
            <span class="cp-name cp-container">{node.name}</span>
            {#if node.auth && node.auth !== 'Inherit'}
              <Badge variant="tone" tone="accent" size="sm" tooltip={`Auth: ${node.auth}`}>{#snippet icon()}<Key size={10} />{/snippet}{node.auth}</Badge>
            {/if}
            <span class="cp-spacer"></span>
            <span class="cp-acts">
              <IconButton tooltip="New request here" size={20} onclick={(e) => { e.stopPropagation(); perform('new-request', node); }}><FilePlus2 size={12} /></IconButton>
              <IconButton tooltip="New folder" size={20} onclick={(e) => { e.stopPropagation(); perform('new-folder', node); }}><FolderPlus size={12} /></IconButton>
              <IconButton tooltip="Delete" shortcut="Del" size={20} variant="danger" onclick={(e) => { e.stopPropagation(); perform('delete', node); }}><Trash2 size={12} /></IconButton>
            </span>
            <span class="cp-count">{countOf(node)}</span>
          {/if}
        {/snippet}
        {#snippet emptyState()}
          {#if query.trim()}
            <EmptyState message="No request matches" compact />
          {:else}
            <EmptyState message="No collections yet" description="Create one, or paste a cURL command to start from a request." compact />
            <div class="cp-cta">
              <Button variant="primary" onclick={hirundoCommands.newCollection}>New collection</Button>
              <Button variant="outline" onclick={hirundoCommands.curl}>Paste cURL…</Button>
            </div>
          {/if}
        {/snippet}
      </Tree>
    </div>
  {/if}
</div>

<style>
  .cp { display: flex; flex-direction: column; min-height: 0; flex: 1; }
  .cp-top { display: flex; gap: 6px; align-items: center; padding: 2px 8px 8px; }
  .cp-top > :global(:first-child) { flex: 1; min-width: 0; }
  .cp-tree { flex: 1; min-height: 0; overflow: auto; padding: 0 4px 8px; }
  .cp-cta { display: flex; gap: 8px; justify-content: center; padding: 8px; flex-wrap: wrap; }

  .cp-method { font: 600 10px var(--font-mono); width: 30px; flex: none; }
  .cp-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .cp-container { font-weight: 600; color: var(--text-primary); }
  .cp-bad { color: var(--error); }
  .cp-folder { color: var(--text-secondary); }
  .cp-collection { color: var(--accent); }
  .cp-spacer { flex: 1; }
  .cp-count { font-size: var(--font-size-xs); color: var(--text-muted); padding-left: 6px; }

  /* Hover actions: out of the way until the pointer is on the row (or the row has focus). */
  .cp-acts { display: none; align-items: center; gap: 1px; }
  :global(.tree-row:hover) .cp-acts, :global(.tree-row:focus-within) .cp-acts { display: inline-flex; }
  :global(.tree-row:hover) .cp-count, :global(.tree-row:focus-within) .cp-count { display: none; }
</style>
