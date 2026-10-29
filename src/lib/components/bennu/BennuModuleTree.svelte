<script lang="ts">
  /**
   * BennuModuleTree — the modules a tool window can act on, grouped by project.
   *
   * Maven, Cargo, Dependencies and Tests each show the modules of THEIR ecosystem inside the ACTIVE
   * project (a Tauri app's `src-tauri`, a monorepo's services); this is the selector that says which
   * one a command applies to. Other projects of the workspace are not listed — switch to them instead. Selecting a node pins
   * it (selecting it again lets go); with nothing pinned the window follows the module that holds
   * the active file — see `moduleFocusStore`.
   *
   * Draws NOTHING when there is only one module to choose from: a selector with a single option is
   * chrome that answers no question, and every single-ecosystem project would pay for it.
   *
   * Projects appear as a level only when the workspace has more than one carrying this ecosystem;
   * otherwise the modules are a flat list.
   *
   * Keyboard comes with the shared Tree: Tab / arrows to a row, Enter or Space pins it.
   */
  import { Boxes, Package } from 'lucide-svelte';
  import Tree, { type RowSnippetCtx } from '$lib/components/shared/ui/Tree.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { moduleFocusStore } from '$lib/stores/bennu/module-focus.svelte';
  import { groupByProject, type ModuleRef } from './project-modules';
  import type { ModuleKind } from '$lib/ipc/bennu/modules';

  let { kinds, ariaLabel = 'Modules' }: { kinds: ModuleKind[]; ariaLabel?: string } = $props();

  interface Node {
    id: string;
    label: string;
    /** Set on a module row; absent on a project row. */
    module?: ModuleRef;
    children?: Node[];
  }

  const refs = $derived(projectStore.modulesOf(...kinds));
  const target = $derived(moduleFocusStore.targetFor(...kinds));
  const groups = $derived(groupByProject(refs));

  function moduleLabel(m: ModuleRef): string {
    return m.dir === '.' ? 'root' : m.dir;
  }

  const nodes = $derived.by<Node[]>(() => {
    const asNode = (m: ModuleRef): Node => ({ id: m.id, label: moduleLabel(m), module: m });
    if (groups.length <= 1) return refs.map(asNode);
    return groups.map((g) => ({
      id: `project:${g.root}`,
      label: projectStore.projectNameOf(g.root),
      children: g.modules.map(asNode),
    }));
  });

  const defaultExpanded = $derived(nodes.filter((n) => n.children).map((n) => n.id));

  function pick(node: Node) {
    if (node.module) moduleFocusStore.toggle(node.module.id);
  }
</script>

{#if refs.length > 1}
  <div class="mt">
    <Tree
      {nodes}
      {defaultExpanded}
      selectedId={target?.id ?? null}
      selectable={(n) => !!n.module}
      onSelect={pick}
      rowTitle={(n) => n.module?.toolRoot}
      guides
      {ariaLabel}
    >
      {#snippet row({ node }: RowSnippetCtx<Node>)}
        <span class="tree-icon">
          {#if node.module}<Package size={12} />{:else}<Boxes size={12} />{/if}
        </span>
        <span class="mt-name">{node.label}</span>
        {#if node.module && moduleFocusStore.selectedId === node.module.id}
          <span class="mt-pin">pinned</span>
        {/if}
      {/snippet}
    </Tree>
  </div>
{/if}

<style>
  .mt { padding: 2px 0 6px; border-bottom: 1px solid var(--border-subtle); }
  .mt-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mt-pin {
    margin-left: auto;
    font-size: var(--font-size-3xs);
    color: var(--text-disabled);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
</style>
