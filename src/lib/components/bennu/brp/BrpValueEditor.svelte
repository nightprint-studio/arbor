<script lang="ts">
  /**
   * One JSON value from a running game, editable in place — recursive over structs, arrays and enums.
   *
   * The editor is **value-driven** and the registry only refines it (see `brp-value.ts`): the value's
   * own JSON type picks the control, and the shape adds what the value cannot say — that a number is
   * an integer, which variants an enum has, and how a child is addressed in a reflect path.
   *
   * Every edit is reported as `(path, newValue)` through `onCommit`, where `path` is the reflect path
   * `world.mutate_components` wants. Controls commit on change/blur/Enter, never per keystroke, so a
   * half-typed number is never sent to a running game.
   */
  import { LocateFixed } from 'lucide-svelte';
  import Self from './BrpValueEditor.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import type { BrpShape } from '$lib/ipc/bennu/brp';
  import {
    currentVariant, elementLabel, elementPath, elementShape, fieldShape, isInlineNumbers, jsonKind,
    memberPath, newtypePath, payloadShape, variantOf,
  } from './brp-value';

  let {
    value,
    shape,
    path,
    readonly = false,
    onCommit,
    onJump,
  }: {
    value: unknown;
    shape?: BrpShape;
    /** Reflect path of this value inside its component/resource; `''` is the root. */
    path: string;
    readonly?: boolean;
    onCommit: (path: string, value: unknown) => void;
    /** Select the entity this value refers to. */
    onJump?: (entity: number) => void;
  } = $props();

  const kind = $derived(jsonKind(value));
  const isInt = $derived(shape?.kind === 'int');
  const variantName = $derived(shape?.kind === 'enum' ? currentVariant(value) : null);
  const variant = $derived(variantOf(shape, variantName));
  const variantOptions = $derived(
    shape?.kind === 'enum' ? shape.variants.map((v) => ({ value: v.name, label: v.name })) : [],
  );
  /** A newtype variant keeps its payload at `.0`; a tuple or struct variant is addressed from the enum. */
  const payloadPath = $derived(variant?.kind === 'tuple' && variant.items.length === 1 ? newtypePath(path) : path);
  const payload = $derived(
    variantName && value && typeof value === 'object' && !Array.isArray(value)
      ? (value as Record<string, unknown>)[variantName]
      : undefined,
  );

  const entries = $derived(
    kind === 'object' ? Object.entries(value as Record<string, unknown>) : [],
  );
  const items = $derived(kind === 'array' ? (value as unknown[]) : []);

  function pickVariant(name: string) {
    const v = variantOf(shape, name);
    if (v) onCommit(path, v.sample);
  }
</script>

{#if shape?.kind === 'enum' && variantName !== null}
  <div class="ve-enum">
    <Select
      value={variantName}
      options={variantOptions}
      size="sm"
      disabled={readonly}
      ariaLabel="Variant"
      onchange={pickVariant}
    />
    {#if payload !== undefined}
      <div class="ve-nest">
        <Self value={payload} shape={payloadShape(variant)} path={payloadPath} {readonly} {onCommit} {onJump} />
      </div>
    {/if}
  </div>
{:else if kind === 'bool'}
  <Toggle checked={value as boolean} size="sm" disabled={readonly} ariaLabel="Value" onchange={(v) => onCommit(path, v)} />
{:else if kind === 'number'}
  <div class="ve-inline">
    <NumberStepper
      value={value as number}
      step={isInt || (shape === undefined && Number.isInteger(value)) ? 1 : 0.1}
      size="sm"
      {readonly}
      ariaLabel="Value"
      onchange={(v) => onCommit(path, v)}
    />
    {#if shape?.kind === 'entity' && onJump}
      <IconButton tooltip="Select this entity" size={22} onclick={() => onJump(value as number)}>
        <LocateFixed size={12} />
      </IconButton>
    {/if}
  </div>
{:else if kind === 'string'}
  <Input value={value as string} size="sm" {readonly} ariaLabel="Value" onchange={(v) => onCommit(path, v)} />
{:else if kind === 'array'}
  {#if isInlineNumbers(value)}
    <div class="ve-axes">
      {#each value as n, i (i)}
        <span class="ve-axis">
          <span class="ve-key">{elementLabel(shape, i)}</span>
          <NumberStepper
            value={n}
            step={0.1}
            size="sm"
            {readonly}
            ariaLabel={elementLabel(shape, i)}
            onchange={(v) => onCommit(elementPath(path, shape, i), v)}
          />
        </span>
      {/each}
    </div>
  {:else if items.length === 0}
    <span class="ve-muted">empty list</span>
  {:else}
    <div class="ve-list">
      {#each items as item, i (i)}
        <div class="ve-row">
          <span class="ve-key">{elementLabel(shape, i)}</span>
          <Self value={item} shape={elementShape(shape, i)} path={elementPath(path, shape, i)} {readonly} {onCommit} {onJump} />
        </div>
      {/each}
    </div>
  {/if}
{:else if kind === 'object'}
  {#if entries.length === 0}
    <span class="ve-muted">empty</span>
  {:else}
    <div class="ve-list">
      {#each entries as [key, member] (key)}
        <div class="ve-row">
          <span class="ve-key">{key}</span>
          <Self value={member} shape={fieldShape(shape, key)} path={memberPath(path, key)} {readonly} {onCommit} {onJump} />
        </div>
      {/each}
    </div>
  {/if}
{:else}
  <span class="ve-muted">null</span>
{/if}

<style>
  .ve-inline { display: inline-flex; align-items: center; gap: 4px; }
  .ve-enum { display: flex; flex-direction: column; gap: 4px; align-items: flex-start; }
  .ve-nest { padding-left: 10px; border-left: 1px solid var(--border-subtle); }
  .ve-list { display: flex; flex-direction: column; gap: 4px; }
  .ve-row { display: flex; align-items: flex-start; gap: 8px; }
  .ve-key {
    flex: none; min-width: 72px; padding-top: 3px;
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-muted);
  }
  .ve-axes { display: flex; flex-wrap: wrap; gap: 8px; }
  .ve-axis { display: inline-flex; align-items: center; gap: 4px; }
  .ve-axis .ve-key { min-width: 0; padding-top: 0; }
  .ve-muted { font-size: var(--font-size-xs); color: var(--text-disabled); font-style: italic; }
</style>
