<script lang="ts">
  /**
   * One component (or resource) of the inspected game: its name, its value as typed editors, and the
   * raw-JSON editor behind a toggle.
   *
   * The same card serves entities and resources — what differs is only *where an edit goes*, and that
   * arrives as callbacks, so this file never learns the difference. A root-level edit (`path === ''`,
   * e.g. a newtype `Score(3)`) has no field to address and is sent as a whole-value replacement.
   */
  import { Braces, ChevronDown, ChevronRight, Trash2 } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Card from '$lib/components/shared/ui/Card.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { bennuBrpStore } from '$lib/stores/bennu/brp.svelte';
  import type { BrpShapeInfo } from '$lib/ipc/bennu/brp';
  import BrpJsonEditor from './BrpJsonEditor.svelte';
  import BrpValueEditor from './BrpValueEditor.svelte';
  import { shortTypeName } from './brp-value';

  let {
    typePath,
    value,
    error,
    readonly = false,
    onMutate,
    onReplace,
    onRemove,
    onJump,
  }: {
    typePath: string;
    value: unknown;
    /** The game could not serialise this one: shown instead of a value. */
    error?: unknown;
    readonly?: boolean;
    /** A field edit: reflect path + new value. */
    onMutate: (path: string, value: unknown) => void;
    /** A whole-value replacement. Resolves `false` when the game refused. */
    onReplace: (value: unknown) => Promise<boolean>;
    onRemove?: () => void;
    onJump?: (entity: number) => void;
  } = $props();

  let expanded = $state(true);
  let raw = $state(false);
  let info = $state<BrpShapeInfo | null>(null);

  $effect(() => {
    const path = typePath;
    void bennuBrpStore.shapeOf(path).then((s) => { if (path === typePath) info = s; });
  });

  const errorText = $derived(
    error && typeof error === 'object' && 'message' in error ? String((error as { message: unknown }).message) : String(error ?? ''),
  );

  function commit(path: string, v: unknown) {
    if (path === '') void onReplace(v);
    else onMutate(path, v);
  }
</script>

<Card variant="elevated" padding="sm">
  {#snippet header()}
    <Button
      variant="ghost"
      size="xs"
      ariaExpanded={expanded}
      tooltip={{ content: typePath }}
      onclick={() => (expanded = !expanded)}
    >
      {#snippet iconStart()}{#if expanded}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if}{/snippet}
      {shortTypeName(typePath)}
    </Button>
  {/snippet}
  {#snippet actions()}
    {#if !error && !readonly}
      <IconButton tooltip={raw ? 'Typed editor' : 'Edit as JSON'} size={22} active={raw} onclick={() => (raw = !raw)}>
        <Braces size={12} />
      </IconButton>
    {/if}
    {#if onRemove && !readonly}
      <IconButton tooltip="Remove" variant="danger" size={22} onclick={onRemove}>
        <Trash2 size={12} />
      </IconButton>
    {/if}
  {/snippet}

  {#if expanded}
    {#if error}
      <Alert variant="warning" compact text={errorText || 'The game could not serialise this value.'} />
    {:else if raw}
      <BrpJsonEditor {value} onApply={onReplace} onCancel={() => (raw = false)} />
    {:else}
      <BrpValueEditor {value} shape={info?.shape} path="" {readonly} onCommit={commit} {onJump} />
    {/if}
  {/if}
</Card>
