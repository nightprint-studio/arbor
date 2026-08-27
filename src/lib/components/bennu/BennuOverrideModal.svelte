<script lang="ts">
  /**
   * "Implement / override methods" — pick which inherited methods to write.
   *
   * The list comes from the backend (it needs the whole supertype hierarchy) grouped by the type
   * that declares each method, which is how you actually think about it: *these* come from the
   * interface I just implemented, *those* from the abstract class above it.
   *
   * ## What is ticked when it opens
   *
   * Everything abstract, and nothing else. Abstract methods are the ones the compiler will demand —
   * opening with them selected makes "implement this interface" a single gesture. A concrete method
   * is a choice: overriding it changes behaviour that already works, so it is never pre-ticked.
   *
   * Keyboard-first: the filter is focused on open, ↑↓ move through the rows, Space toggles the row
   * under the cursor, Ctrl/Cmd+Enter writes, Esc cancels (the Modal owns it).
   *
   * Composed of shared widgets only — Modal/Header/Footer, Checkbox, Input, Button, EmptyState,
   * Spinner, Badge.
   */
  import { Check, Wand2 } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Checkbox from '$lib/components/shared/ui/Checkbox.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import type { OverridableMember } from '$lib/ipc/bennu/overrides';

  let {
    members,
    loading = false,
    onClose,
    onGenerate,
  }: {
    /** The candidates, abstract first (the backend's order). */
    members: OverridableMember[];
    loading?: boolean;
    onClose: () => void;
    /** Called with the ticked methods, in the order they were offered. */
    onGenerate: (selected: OverridableMember[]) => void;
  } = $props();

  /** A row's identity: the signature is unique within a declaring type, and two types may both
   *  declare `toString()`. */
  const keyOf = (m: OverridableMember) => `${m.declaring_type}#${m.signature}`;

  // Abstract ones start ticked — see the component docs.
  let picked = $state(new Set(members.filter((m) => m.is_abstract).map(keyOf)));
  let filter = $state('');
  let cursor = $state(0);

  const visible = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return members;
    return members.filter(
      (m) =>
        m.signature.toLowerCase().includes(q) ||
        m.declaring_type.toLowerCase().includes(q),
    );
  });

  /** The visible rows, grouped by declaring type in the order the backend returned them. */
  const groups = $derived.by(() => {
    const out: { type: string; rows: OverridableMember[] }[] = [];
    for (const m of visible) {
      const last = out[out.length - 1];
      if (last && last.type === m.declaring_type) last.rows.push(m);
      else out.push({ type: m.declaring_type, rows: [m] });
    }
    return out;
  });

  const selected = $derived(members.filter((m) => picked.has(keyOf(m))));

  function toggle(m: OverridableMember) {
    const k = keyOf(m);
    const next = new Set(picked);
    if (next.has(k)) next.delete(k);
    else next.add(k);
    picked = next;
  }

  /** Tick or untick a whole supertype's methods at once — the group header's box. */
  function toggleGroup(rows: OverridableMember[], on: boolean) {
    const next = new Set(picked);
    for (const m of rows) {
      if (on) next.add(keyOf(m));
      else next.delete(keyOf(m));
    }
    picked = next;
  }

  const groupState = (rows: OverridableMember[]) => {
    const on = rows.filter((m) => picked.has(keyOf(m))).length;
    return { all: on === rows.length && on > 0, some: on > 0 && on < rows.length };
  };

  /** The simple name of a dotted FQCN — the group header reads `Comparable`, not the package. */
  const simple = (fqcn: string) => fqcn.split('.').pop() ?? fqcn;

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      if (selected.length) onGenerate(selected);
      return;
    }
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const step = e.key === 'ArrowDown' ? 1 : -1;
      cursor = Math.max(0, Math.min(visible.length - 1, cursor + step));
      return;
    }
    if (e.key === ' ' && visible[cursor]) {
      e.preventDefault();
      toggle(visible[cursor]);
    }
  }

  // A filter that shrinks the list must not leave the cursor pointing past the end.
  $effect(() => {
    if (cursor >= visible.length) cursor = Math.max(0, visible.length - 1);
  });

  let filterEl = $state<HTMLInputElement | undefined>(undefined);
  $effect(() => { filterEl?.focus(); });

  const indexOfRow = (m: OverridableMember) => visible.indexOf(m);
</script>

<Modal width="640px" height="560px" {onClose}>
  <ModalHeader title="Implement / override methods" icon={Wand2} {onClose} />

  <div class="ov-body" onkeydown={onKeydown} role="presentation">
    <Input
      bind:element={filterEl}
      bind:value={filter}
      placeholder="Filter by name or declaring type…"
      ariaLabel="Filter methods"
    />

    {#if loading}
      <div class="ov-state"><Spinner size={16} /> <span>Reading the hierarchy…</span></div>
    {:else if members.length === 0}
      <EmptyState
        title="Nothing to override"
        description="This class inherits no method it is allowed to override — or its index is still building."
      />
    {:else if visible.length === 0}
      <EmptyState title="No match" description="No inherited method matches that filter." />
    {:else}
      <div class="ov-list">
        {#each groups as g (g.type)}
          {@const state = groupState(g.rows)}
          <div class="ov-group">
            <div class="ov-group-head">
              <Checkbox
                checked={state.all}
                indeterminate={state.some}
                ariaLabel={`Select every method from ${simple(g.type)}`}
                onchange={(on) => toggleGroup(g.rows, on)}
              />
              <span class="ov-group-name" title={g.type}>{simple(g.type)}</span>
              <span class="ov-group-pkg">{g.type}</span>
            </div>
            {#each g.rows as m (keyOf(m))}
              <div class="ov-row" class:at={indexOfRow(m) === cursor}>
                <Checkbox
                  checked={picked.has(keyOf(m))}
                  ariaLabel={m.signature}
                  onchange={() => toggle(m)}
                />
                <span class="ov-sig">{m.signature}</span>
                {#if m.is_abstract}
                  <Badge variant="tone" tone="warning" size="sm" label="abstract" />
                {/if}
              </div>
            {/each}
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <ModalFooter>
    <span class="ov-count">
      {selected.length} of {members.length} selected
    </span>
    <Button variant="ghost" onclick={onClose}>Cancel</Button>
    <Button
      variant="primary"
      disabled={selected.length === 0}
      onclick={() => onGenerate(selected)}
    >
      {#snippet iconStart()}<Check size={13} />{/snippet}
      Generate
    </Button>
  </ModalFooter>
</Modal>

<style>
  .ov-body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 14px;
    min-height: 0;
    flex: 1;
  }
  .ov-state {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-muted);
    font-size: 12px;
    padding: 20px 0;
  }
  .ov-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--bg-base);
  }
  .ov-group + .ov-group {
    border-top: 1px solid var(--border-subtle);
  }
  .ov-group-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    background: var(--bg-elevated);
    position: sticky;
    top: 0;
    z-index: 1;
  }
  .ov-group-name {
    font-weight: 600;
    font-size: 12px;
  }
  .ov-group-pkg {
    font-size: 11px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ov-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 10px 4px 22px;
  }
  .ov-row.at {
    background: var(--bg-hover);
  }
  .ov-sig {
    font-family: var(--font-mono);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }
  .ov-count {
    margin-right: auto;
    font-size: 12px;
    color: var(--text-muted);
  }
</style>
