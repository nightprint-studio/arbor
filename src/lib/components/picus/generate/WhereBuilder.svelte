<script lang="ts">
  /**
   * The `WHERE` of an update or a delete, built as a tree.
   *
   * ## One node, drawn recursively
   *
   * A condition is a row: column, operator, and as many value boxes as the
   * operator takes. A group is a bracket around more of them with `AND` / `OR` on
   * its edge. The component renders **itself** for the children, which is what
   * keeps `(A AND (B OR C))` a shape you can see rather than a string you have to
   * parse in your head.
   *
   * ## Why not a text box
   *
   * A free-text WHERE would be one input and no work — and the point at which
   * Picus stops knowing what the script does. Every rule in this product rests on
   * the model being structured; a hole here is a hole in the guarantee, not in one
   * feature. Where a *value* genuinely needs SQL, it carries it: the operand boxes
   * take the same `=` prefix as every other DML value, so `=SYSDATE` and
   * `=(SELECT …)` are one keystroke away without the clause itself becoming opaque.
   */
  // Self-import rather than `<svelte:self>`, which Svelte 5 deprecates: a
  // component referring to itself by name is the supported form, and it is the
  // one that keeps working when this file is renamed.
  import Self from './WhereBuilder.svelte';
  import { Plus, Trash2, Parentheses } from 'lucide-svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import {
    operandArity,
    PREDICATE_OPERATORS,
    type Column,
    type Predicate,
    type PredicateJoin,
    type PredicateOperator,
  } from '$lib/types/picus';

  interface Props {
    node: Predicate;
    /** The table's columns, for the picker. */
    columns: Column[];
    /** Replaced with the edited node — the parent writes it back. */
    onChange: (next: Predicate) => void;
    /** Absent on the root, which cannot be removed. */
    onRemove?: () => void;
    depth?: number;
  }

  let { node, columns, onChange, onRemove, depth = 0 }: Props = $props();

  const columnOptions = $derived([
    { value: '', label: 'Choose a column' },
    ...columns.map((c) => ({ value: c.name, label: c.name })),
  ]);

  function newCondition(): Predicate {
    return { kind: 'condition', column: '', operator: 'equals', operands: [''] };
  }

  /** Changing the operator resizes the operand list to what it takes, keeping what
   *  was typed where it still applies — retyping a value because you switched from
   *  `=` to `<>` is the kind of small rudeness that makes a form unusable. */
  function setOperator(operator: PredicateOperator) {
    if (node.kind !== 'condition') return;
    const wanted = operandArity(operator);
    const kept = node.operands.filter((o) => o.trim());
    const operands =
      wanted === 'none' ? []
      : wanted === 'one' ? [kept[0] ?? '']
      : wanted === 'two' ? [kept[0] ?? '', kept[1] ?? '']
      : kept.length ? kept : [''];
    onChange({ ...node, operator, operands });
  }

  function setOperand(index: number, value: string) {
    if (node.kind !== 'condition') return;
    const operands = [...node.operands];
    operands[index] = value;
    onChange({ ...node, operands });
  }

  function replaceChild(index: number, next: Predicate) {
    if (node.kind !== 'group') return;
    const of = [...node.of];
    of[index] = next;
    onChange({ ...node, of });
  }

  function removeChild(index: number) {
    if (node.kind !== 'group') return;
    onChange({ ...node, of: node.of.filter((_, i) => i !== index) });
  }

  function add(child: Predicate) {
    if (node.kind !== 'group') return;
    onChange({ ...node, of: [...node.of, child] });
  }
</script>

{#if node.kind === 'group'}
  <div class="wb-group" style:--wb-depth={depth}>
    <div class="wb-join">
      <!-- On the edge of the bracket rather than between the rows: it is a
           property of the GROUP, and a word repeated between every pair reads as
           though each pair could differ. -->
      <Select
        value={node.join}
        options={[
          { value: 'and', label: 'AND' },
          { value: 'or', label: 'OR' },
        ]}
        onchange={(v) => onChange({ ...node, join: v as PredicateJoin })}
      />
      {#if onRemove}
        <Button
          variant="icon"
          size="xs"
          ariaLabel="Remove this group"
          tooltip={'Remove this group and everything in it'}
          onclick={onRemove}
        >
          {#snippet iconStart()}<Trash2 size={12} />{/snippet}
        </Button>
      {/if}
    </div>

    <div class="wb-children">
      {#each node.of as child, i (i)}
        <Self
          node={child}
          {columns}
          depth={depth + 1}
          onChange={(next) => replaceChild(i, next)}
          onRemove={() => removeChild(i)}
        />
      {:else}
        <p class="wb-empty">
          Nothing yet — with no condition at all this matches the comparison key.
        </p>
      {/each}

      <div class="wb-add">
        <Button variant="ghost" size="xs" onclick={() => add(newCondition())}>
          {#snippet iconStart()}<Plus size={12} />{/snippet}
          Condition
        </Button>
        <Button
          variant="ghost"
          size="xs"
          tooltip={'A bracket — its own AND or OR, so (A AND B) OR C is expressible'}
          onclick={() => add({ kind: 'group', join: node.join === 'and' ? 'or' : 'and', of: [newCondition()] })}
        >
          {#snippet iconStart()}<Parentheses size={12} />{/snippet}
          Group
        </Button>
      </div>
    </div>
  </div>
{:else}
  {@const arity = operandArity(node.operator)}
  <div class="wb-row">
    <span class="wb-col">
      <Select
        value={node.column}
        options={columnOptions}
        searchable={columns.length > 8}
        onchange={(v) => onChange({ ...node, column: v })}
      />
    </span>

    <span class="wb-op">
      <Select
        value={node.operator}
        options={PREDICATE_OPERATORS.map((o) => ({ value: o.id, label: o.label }))}
        onchange={(v) => setOperator(v as PredicateOperator)}
      />
    </span>

    <span class="wb-operands">
      {#if arity === 'none'}
        <span class="wb-none">no value</span>
      {:else if arity === 'two'}
        <Input
          value={node.operands[0] ?? ''}
          size="sm"
          ariaLabel="Lower bound"
          placeholder="from"
          oninput={(v) => setOperand(0, String(v))}
        />
        <span class="wb-and">and</span>
        <Input
          value={node.operands[1] ?? ''}
          size="sm"
          ariaLabel="Upper bound"
          placeholder="to"
          oninput={(v) => setOperand(1, String(v))}
        />
      {:else if arity === 'many'}
        {#each node.operands as operand, i (i)}
          <Input
            value={operand}
            size="sm"
            ariaLabel={`Value ${i + 1}`}
            placeholder="value"
            oninput={(v) => setOperand(i, String(v))}
          />
        {/each}
        <Button
          variant="icon"
          size="xs"
          ariaLabel="Add a value to the list"
          tooltip={'Add a value'}
          onclick={() => onChange({ ...node, operands: [...node.operands, ''] })}
        >
          {#snippet iconStart()}<Plus size={12} />{/snippet}
        </Button>
      {:else}
        <Input
          value={node.operands[0] ?? ''}
          size="sm"
          ariaLabel="Value"
          placeholder="value — = for SQL"
          oninput={(v) => setOperand(0, String(v))}
        />
      {/if}
    </span>

    {#if onRemove}
      <Button variant="icon" size="xs" ariaLabel="Remove this condition" onclick={onRemove}>
        {#snippet iconStart()}<Trash2 size={12} />{/snippet}
      </Button>
    {/if}
  </div>
{/if}

<style>
  /* The bracket, drawn. A group nested in a group has to be visible as one, and a
     left border does it with no chrome. */
  .wb-group {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 4px 0 4px 6px;
    border-left: 2px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .wb-group:not(:first-child) { margin-top: 2px; }

  .wb-join { display: flex; align-items: center; gap: 3px; flex-shrink: 0; width: 88px; }

  .wb-children { display: flex; flex-direction: column; gap: 3px; flex: 1; min-width: 0; }

  .wb-row { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .wb-col { min-width: 150px; flex: 1 1 180px; }
  .wb-op { min-width: 110px; flex: 0 0 auto; }
  .wb-operands {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 2 1 240px;
    min-width: 0;
    flex-wrap: wrap;
  }
  .wb-and,
  .wb-none { font-size: 11px; color: var(--text-muted); }

  .wb-add { display: flex; gap: 4px; padding-top: 2px; }

  .wb-empty { font-size: 11px; color: var(--text-muted); padding: 2px 0; }
</style>
