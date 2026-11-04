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
  import { brpHintsStore } from '$lib/stores/bennu/brp-hints.svelte';
  import BrpColorField from './BrpColorField.svelte';
  import BrpDurationField from './BrpDurationField.svelte';
  import BrpQuatField from './BrpQuatField.svelte';
  import BrpScalarField from './BrpScalarField.svelte';
  import BrpTransformField from './BrpTransformField.svelte';
  import BrpVectorField from './BrpVectorField.svelte';
  import { classify, hintFor } from './brp-widgets';
  import {
    currentVariant, elementLabel, elementPath, elementShape, fieldShape, isInlineNumbers, jsonKind,
    memberPath, newtypePath, payloadShape, variantOf,
  } from './brp-value';

  let {
    value,
    shape,
    path,
    readonly = false,
    quiet = false,
    rootType,
    linear = false,
    onCommit,
    onJump,
  }: {
    value: unknown;
    shape?: BrpShape;
    /** Reflect path of this value inside its component/resource; `''` is the root. */
    path: string;
    readonly?: boolean;
    /** Fields drawn without a frame until hovered or focused — the inspector's reading mode. */
    quiet?: boolean;
    /** Type path of the component this value belongs to: the key its `#[inspect]` hints are stored under. */
    rootType?: string;
    /** The channels of a colour are linear light (the enclosing enum variant said so). */
    linear?: boolean;
    onCommit: (path: string, value: unknown) => void;
    /** Select the entity this value refers to. */
    onJump?: (entity: number) => void;
  } = $props();

  const kind = $derived(jsonKind(value));
  const hint = $derived(hintFor(brpHintsStore.hints, rootType, path));
  const ro = $derived(readonly || !!hint?.readonly);
  const widget = $derived(classify(shape?.kind === 'struct' ? shape.type_path : rootType && path === '' ? rootType : undefined, value, hint));
  const hintOf = (key: string) => hintFor(brpHintsStore.hints, rootType, memberPath(path, key));
  const labelOf = (key: string) => hintOf(key)?.label ?? key;
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

{#if widget === 'transform'}
  <BrpTransformField value={value as { translation: number[]; rotation: number[]; scale: number[] }} {path} readonly={ro} {onCommit} />
{:else if widget === 'color'}
  <BrpColorField {value} {path} {linear} readonly={ro} {onCommit} />
{:else if widget === 'quat'}
  <BrpQuatField value={value as number[]} {path} readonly={ro} {onCommit} />
{:else if widget === 'vec'}
  <BrpVectorField value={value as number[]} {path} readonly={ro} {hint} {onCommit} />
{:else if widget === 'duration'}
  <BrpDurationField value={value as { secs: number; nanos: number }} {path} readonly={ro} {onCommit} />
{:else if widget === 'handle'}
  <span class="ve-handle" title={JSON.stringify(value)}>{Object.keys(value as object)[0]} handle</span>
{:else if widget === 'slider' || widget === 'angle' || widget === 'date'}
  <BrpScalarField kind={widget} value={value as number} {path} {hint} readonly={ro} {onCommit} />
{:else if shape?.kind === 'enum' && variantName !== null}
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
        <Self value={payload} shape={payloadShape(variant)} path={payloadPath} {readonly} {quiet} {rootType} linear={variantName === 'LinearRgba'} {onCommit} {onJump} />
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
      {quiet}
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
  <Input value={value as string} size="sm" {readonly} {quiet} ariaLabel="Value" onchange={(v) => onCommit(path, v)} />
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
            {quiet}
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
          <Self value={item} shape={elementShape(shape, i)} path={elementPath(path, shape, i)} {readonly} {quiet} {rootType} {onCommit} {onJump} />
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
          <span class="ve-labelcol">
            <span class="ve-key" title={hintOf(key)?.description}>{labelOf(key)}</span>
            {#if hintOf(key)?.description}<span class="ve-help">{hintOf(key)?.description}</span>{/if}
          </span>
          <Self value={member} shape={fieldShape(shape, key)} path={memberPath(path, key)} {readonly} {quiet} {rootType} linear={linear || key === 'LinearRgba'} {onCommit} {onJump} />
        </div>
      {/each}
    </div>
  {/if}
{:else}
  <span class="ve-muted">null</span>
{/if}

<style>
  .ve-inline { display: inline-flex; align-items: center; gap: 4px; }
  .ve-enum { display: flex; flex-direction: column; gap: 2px; align-items: flex-start; }
  .ve-nest { padding-left: 10px; border-left: 1px solid var(--border-subtle); }
  .ve-list { display: flex; flex-direction: column; gap: 1px; }
  .ve-row { display: flex; align-items: flex-start; gap: 8px; min-height: 22px; }
  .ve-labelcol { flex: none; min-width: 84px; max-width: 180px; display: flex; flex-direction: column; }
  .ve-help { font-size: var(--font-size-2xs); line-height: 1.25; color: var(--text-disabled); }
  .ve-key {
    flex: none; min-width: 0; line-height: 22px;
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-muted);
  }
  .ve-axes { display: flex; flex-wrap: wrap; gap: 4px 10px; }
  .ve-axis { display: inline-flex; align-items: center; gap: 4px; }
  .ve-axis .ve-key { min-width: 0; padding-top: 0; }
  .ve-handle { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); padding: 1px 6px; border-radius: 8px; background: var(--bg-overlay); }
  .ve-muted { font-size: var(--font-size-xs); color: var(--text-disabled); font-style: italic; }
</style>
