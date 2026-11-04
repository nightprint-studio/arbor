<script lang="ts">
  /**
   * One component (or resource) of the inspected game, as a flat section: a header line with its name
   * and — while closed — a one-line summary of its value, and the typed editors beneath when open.
   *
   * A lone number, string or flag has no body to open: it is edited on the header line itself. The
   * raw-JSON editor and "remove" appear on hover (or focus), so a screen of components is a column of
   * quiet lines rather than a stack of boxes.
   *
   * The same section serves entities and resources — what differs is only *where an edit goes*, and
   * that arrives as callbacks. A root-level edit (`path === ''`, e.g. a newtype `Score(3)`) has no field
   * to address and is sent as a whole-value replacement.
   */
  import { Braces, Trash2, TriangleAlert } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { bennuBrpStore } from '$lib/stores/bennu/brp.svelte';
  import type { BrpShapeInfo } from '$lib/ipc/bennu/brp';
  import BrpIcon from './icons/BrpIcon.svelte';
  import { componentIcon } from './icons/icon-kinds';
  import BrpJsonEditor from './BrpJsonEditor.svelte';
  import BrpValueEditor from './BrpValueEditor.svelte';
  import { brpHintsStore } from '$lib/stores/bennu/brp-hints.svelte';
  import { hintFor } from './brp-widgets';
  import { isInlineValue, shortTypeName, summarize } from './brp-value';

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

  let open = $state(true);
  let raw = $state(false);
  let info = $state<BrpShapeInfo | null>(null);

  $effect(() => {
    const path = typePath;
    void bennuBrpStore.shapeOf(path).then((s) => { if (path === typePath) info = s; });
  });

  const errorText = $derived(
    error && typeof error === 'object' && 'message' in error ? String((error as { message: unknown }).message) : String(error ?? ''),
  );
  const inline = $derived(!error && !raw && isInlineValue(value));
  const name = $derived(shortTypeName(typePath));
  /** Only the types that have a look of their own get an icon: a column of identical ones says nothing. */
  const icon = $derived(componentIcon(typePath));
  /** What the game says this type is (its `///` docs or `#[inspect(description)]`). */
  const about = $derived(hintFor(brpHintsStore.hints, typePath, '')?.description);

  function commit(path: string, v: unknown) {
    if (path === '') void onReplace(v);
    else onMutate(path, v);
  }
</script>

<section class="vs" class:err={!!error} title={about ? `${typePath}\n${about}` : typePath}>
  {#if inline}
    <div class="vs-line">
      <span class="vs-name">{#if icon !== 'component'}<BrpIcon name={icon} size={12} />{/if}{name}</span>
      <div class="vs-inline">
        <BrpValueEditor {value} shape={info?.shape} path="" {readonly} quiet rootType={typePath} onCommit={commit} {onJump} />
      </div>
    </div>
  {:else}
    <Collapsible bind:open chevron>
      {#snippet header()}
        <span class="vs-name">
          {#if error}<TriangleAlert size={11} />{:else if icon !== 'component'}<BrpIcon name={icon} size={12} />{/if}
          {name}
        </span>
        {#if !open && !error}<span class="vs-summary">{summarize(value)}</span>{/if}
      {/snippet}
      <div class="vs-body">
        {#if about}<p class="vs-about">{about}</p>{/if}
        {#if error}
          <Alert variant="warning" compact text={errorText || 'The game could not serialise this value.'} />
        {:else if raw}
          <BrpJsonEditor {value} onApply={onReplace} onCancel={() => (raw = false)} />
        {:else}
          <BrpValueEditor {value} shape={info?.shape} path="" {readonly} quiet rootType={typePath} onCommit={commit} {onJump} />
        {/if}
      </div>
    </Collapsible>
  {/if}

  <div class="vs-actions">
    {#if !error && !readonly && !inline}
      <IconButton tooltip={raw ? 'Typed editor' : 'Edit as JSON'} size={20} active={raw} onclick={() => { raw = !raw; open = true; }}>
        <Braces size={11} />
      </IconButton>
    {/if}
    {#if onRemove && !readonly}
      <IconButton tooltip="Remove" variant="danger" size={20} onclick={onRemove}>
        <Trash2 size={11} />
      </IconButton>
    {/if}
  </div>
</section>

<style>
  .vs { position: relative; border-bottom: 1px solid var(--border-subtle); padding: 2px 0; }
  .vs:last-child { border-bottom: none; }
  .vs-line { display: flex; align-items: center; gap: 8px; min-height: 24px; padding-left: 18px; padding-right: 48px; }
  .vs-name {
    display: inline-flex; align-items: center; gap: 4px; flex: none;
    font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary);
  }
  .vs.err .vs-name { color: var(--warning); }
  .vs-inline { flex: 1; min-width: 0; display: flex; justify-content: flex-start; }
  .vs-summary {
    min-width: 0; margin-left: 6px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
  }
  .vs-body { padding: 2px 48px 6px 18px; }
  .vs-about { margin: 0 0 4px; font-size: var(--font-size-2xs); line-height: 1.3; color: var(--text-disabled); }
  .vs-actions {
    position: absolute; top: 2px; right: 2px; display: flex; gap: 2px;
    opacity: 0; transition: opacity var(--transition-fast);
  }
  .vs:hover .vs-actions, .vs:focus-within .vs-actions { opacity: 1; }
</style>
