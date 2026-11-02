<script lang="ts">
  /**
   * Small host for overlay widgets that only make sense next to a trigger or an anchor:
   * ContextMenu, Dropdown (open) and Popover. One component, one `kind` prop, so the gallery
   * needs no wrapper file per widget.
   */
  import { GitBranch, Copy, Trash2, Pencil, Upload, Star } from 'lucide-svelte';
  import ContextMenu from '$lib/components/shared/ContextMenu.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import Popover from '$lib/components/shared/ui/Popover.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';

  let { kind }: { kind: 'context-menu' | 'dropdown' | 'dropdown-searchable' | 'popover' } = $props();

  const noop = () => {};

  const menuItems = [
    { id: 'hdr', label: 'Branch', header: true },
    { id: 'co', label: 'Checkout', icon: GitBranch, shortcut: 'Ctrl+Shift+K' },
    { id: 'merge', label: 'Merge into current', icon: GitBranch, subtitle: 'feature/x → main' },
    { id: 'push', label: 'Push', icon: Upload, badge: '2 ahead', badgeAccent: true },
    { id: 'sep', label: '', separator: true },
    { id: 'rename', label: 'Rename…', icon: Pencil, shortcut: 'F2' },
    { id: 'copy', label: 'Copy name', icon: Copy },
    { id: 'more', label: 'Compare with', icon: GitBranch, children: [
      { id: 'cmp1', label: 'main' },
      { id: 'cmp2', label: 'develop' },
    ] },
    { id: 'sep2', label: '', separator: true },
    { id: 'del', label: 'Delete branch', icon: Trash2, danger: true },
    { id: 'dis', label: 'Disabled action', disabled: true },
  ];

  const dropItems = [
    { kind: 'item' as const, id: 'a', label: 'main', icon: GitBranch, active: true, meta: 'HEAD', onclick: noop },
    { kind: 'item' as const, id: 'b', label: 'develop', icon: GitBranch, subtitle: '3 commits behind', onclick: noop },
    { kind: 'item' as const, id: 'c', label: 'feature/legacy-import', icon: GitBranch, shortcut: 'Ctrl+1', onclick: noop },
    { kind: 'item' as const, id: 'd', label: 'release/1.4', icon: Star, danger: false, onclick: noop },
    { kind: 'item' as const, id: 'e', label: 'delete me', icon: Trash2, danger: true, onclick: noop },
    { kind: 'item' as const, id: 'f', label: 'disabled', disabled: true, onclick: noop },
  ];
</script>

{#if kind === 'context-menu'}
  <ContextMenu items={menuItems} x={260} y={120} onSelect={noop} onClose={noop} />
{:else if kind === 'popover'}
  <Popover label="Filters" onclose={noop} align="end" width={320} maxHeight={300}>
    <div style="padding:12px; color:var(--text-secondary); font-size:12px; display:flex; flex-direction:column; gap:8px">
      <strong style="color:var(--text-primary)">Popover body</strong>
      <span>Any content goes here.</span>
      <Button variant="primary" size="sm">Apply</Button>
    </div>
  </Popover>
{:else}
  <div style="padding: 40px 80px">
    <Dropdown
      items={dropItems}
      autoOpen
      searchable={kind === 'dropdown-searchable'}
      searchPlaceholder="Filter branches…"
      width="280px"
    >
      {#snippet trigger({ toggle })}
        <Button variant="outline" size="md" onclick={toggle}>main ▾</Button>
      {/snippet}
    </Dropdown>
  </div>
{/if}
