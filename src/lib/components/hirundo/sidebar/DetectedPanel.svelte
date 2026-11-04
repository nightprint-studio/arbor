<script lang="ts">
  /**
   * DetectedPanel — the endpoints Bennu finds in the code, only for the Bennu projects chosen here (never all
   * of them). Until a choice is made it is the Bennu project that is this window's own. A controller is a
   * container, an endpoint a leaf: Enter or a double click opens it as a new request (method, URL, query
   * parameters and body shape from the code); saving that request puts it in a collection.
   */
  import { untrack } from 'svelte';
  import { ChevronDown, Code2, FileCode2, RefreshCw } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Tree, { type RowSnippetCtx } from '$lib/components/shared/ui/Tree.svelte';
  import { openPath } from '@tauri-apps/plugin-opener';
  import { hirundoContextMenuStore } from '$lib/stores/hirundo/contextmenu.svelte';
  import { endpointToRequest, hirundoDetected } from '$lib/stores/hirundo/detected.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { methodColor, methodShort } from '../request-model';
  import type { DetectedEndpoint, Method } from '$lib/types/hirundo';

  interface Row {
    id: string;
    kind: 'project' | 'controller' | 'endpoint';
    name: string;
    method?: Method;
    path?: string;
    endpoint?: DetectedEndpoint;
    children: Row[];
  }

  $effect(() => {
    // Re-read whenever the project, or the choice of projects, changes.
    void hirundoStore.project;
    void hirundoStore.config.detected_roots;
    untrack(() => void hirundoDetected.refresh());
  });

  const rows = $derived<Row[]>(
    hirundoDetected.groups.map((g) => {
      const byController = new Map<string, DetectedEndpoint[]>();
      for (const e of g.endpoints) byController.set(e.controller, [...(byController.get(e.controller) ?? []), e]);
      return {
        id: `p:${g.root}`, kind: 'project' as const, name: g.name,
        children: [...byController].map(([controller, list]) => ({
          id: `c:${g.root}:${controller}`, kind: 'controller' as const, name: controller,
          children: list.map((e) => ({
            id: `e:${g.root}:${e.id}`, kind: 'endpoint' as const, name: e.handler, method: (e.methods[0]?.toUpperCase() ?? 'GET') as Method, path: e.path, endpoint: e, children: [],
          })),
        })),
      };
    }),
  );

  const count = $derived(hirundoDetected.groups.reduce((n, g) => n + g.endpoints.length, 0));

  const projectItems = $derived<DropdownItem[]>(
    hirundoDetected.candidates.map((c) => ({
      kind: 'item', id: c.root, label: c.name, subtitle: c.workspace, active: hirundoDetected.roots.includes(c.root),
      onclick: () => {
        const next = hirundoDetected.roots.includes(c.root) ? hirundoDetected.roots.filter((r) => r !== c.root) : [...hirundoDetected.roots, c.root];
        void hirundoDetected.choose(next);
      },
    })),
  );

  function openAsRequest(e: DetectedEndpoint) {
    hirundoStore.newRequest(hirundoStore.tree[0]?.path ?? null, endpointToRequest(e));
  }

  function onContext(row: Row, ev: MouseEvent) {
    if (row.kind !== 'endpoint' || !row.endpoint) return;
    ev.preventDefault();
    const e = row.endpoint;
    hirundoContextMenuStore.show(ev.clientX, ev.clientY, [
      { id: 'open', label: 'Open as request', icon: Code2 },
      { id: 'source', label: 'Open the controller source', icon: FileCode2 },
    ], (id) => { if (id === 'open') openAsRequest(e); else void openPath(e.file); });
  }
</script>

<div class="dp">
  <div class="dp-top">
    <Dropdown items={projectItems} selectionMode="multiple" closeOnSelect={false} width="280px" searchable={projectItems.length > 8} emptyMessage="No Bennu project known yet">
      {#snippet trigger({ open, toggle })}
        <Button variant="outline" size="sm" ariaExpanded={open} ariaHaspopup="menu" onclick={toggle}>
          Projects · {hirundoDetected.roots.length}
          {#snippet iconStart()}<ChevronDown size={12} />{/snippet}
        </Button>
      {/snippet}
    </Dropdown>
    <span class="dp-count">{count} endpoints</span>
    <span class="dp-spacer"></span>
    <IconButton tooltip="Read the code again" size={26} onclick={() => void hirundoDetected.refresh()}><RefreshCw size={13} /></IconButton>
  </div>

  {#if hirundoDetected.unavailable}
    <div class="dp-pad"><Alert variant="warning" compact text={hirundoDetected.unavailable} /></div>
  {:else if hirundoDetected.loading && !rows.length}
    <div class="dp-center"><Spinner label="Reading the code…" delay={0} /></div>
  {:else if hirundoDetected.roots.length === 0}
    <EmptyState message="Choose the projects" description="Pick the Bennu projects whose endpoints you want here. Only those are read." />
  {:else if count === 0}
    <EmptyState message="No endpoints found" description="Open the project once in Bennu so its index exists; Spring request mappings are listed from there." />
  {:else}
    <div class="dp-tree">
      <Tree
        nodes={rows}
        hasChildren={(n) => n.kind !== 'endpoint'}
        initialExpanded={() => true}
        indentSize={16}
        basePadding={8}
        rowHeight={26}
        guides
        ariaLabel="Detected endpoints"
        onActivate={(n) => { if (n.endpoint) openAsRequest(n.endpoint); }}
        onSelect={() => {}}
        onContextMenu={onContext}
        rowTitle={(n) => n.endpoint?.file}
      >
        {#snippet row({ node }: RowSnippetCtx<Row>)}
          {#if node.kind === 'endpoint'}
            <span class="dp-method" style:color={methodColor(node.method ?? 'GET')}>{methodShort(node.method ?? 'GET')}</span>
            <span class="dp-path">{node.path}</span>
          {:else}
            <span class="dp-container">{node.name}</span>
          {/if}
        {/snippet}
      </Tree>
    </div>
  {/if}
</div>

<style>
  .dp { display: flex; flex-direction: column; flex: 1; min-height: 0; }
  .dp-top { display: flex; align-items: center; gap: 8px; padding: 2px 8px 8px; }
  .dp-spacer { flex: 1; }
  .dp-count { font-size: var(--font-size-xs); color: var(--text-muted); }
  .dp-pad { padding: 8px; }
  .dp-center { flex: 1; display: grid; place-items: center; }
  .dp-tree { flex: 1; min-height: 0; overflow: auto; padding: 0 4px 8px; }
  .dp-method { font: 600 10px var(--font-mono); width: 30px; flex: none; }
  .dp-path { font-family: var(--font-mono); font-size: var(--font-size-xs); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dp-container { font-weight: 600; color: var(--text-primary); }
</style>
