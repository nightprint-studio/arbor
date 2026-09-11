<script lang="ts">
  /** The object a payload was bound to, field by field, as the JVM found it. */
  import Tree from '$lib/components/shared/ui/Tree.svelte';
  import type { DtoLabObjectNode } from '$lib/ipc/bennu/dtolab';

  let { root }: { root: DtoLabObjectNode } = $props();

  interface Row {
    id: string;
    label: string;
    type: string | null;
    text: string | null;
    children: Row[];
  }

  function toRow(node: DtoLabObjectNode, id: string, label: string): Row {
    return {
      id,
      label,
      type: node.type,
      text: node.text,
      children: node.children.map((child, i) => toRow(child, `${id}/${i}`, child.name ?? `[${i}]`)),
    };
  }

  // The root is the object itself; its fields are what is worth a row each.
  const rows = $derived(
    root.children.length > 0
      ? root.children.map((child, i) => toRow(child, String(i), child.name ?? `[${i}]`))
      : [toRow(root, 'value', root.type ?? 'value')],
  );
</script>

<Tree nodes={rows} getId={(row) => row.id} getChildren={(row) => row.children} initialExpanded={() => true}>
  {#snippet row({ node })}
    <span class="name">{node.label}</span>
    {#if node.type}<span class="type">{node.type}</span>{/if}
    {#if node.text !== null}
      <span class="value" class:null={node.text === 'null'}>{node.text}</span>
    {/if}
  {/snippet}
</Tree>

<style>
  .name { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); }
  .type { font-size: var(--font-size-2xs); color: var(--text-muted); }
  .value {
    min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--accent);
  }
  .value.null { color: var(--text-disabled); }
</style>
