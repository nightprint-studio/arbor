<script lang="ts">
  /**
   * The escape hatch of every typed editor: a value as raw JSON.
   *
   * The registry can describe most types, but not all — and a `Vec3` that travels as an array while
   * being registered as a struct is only the first of many places where a typed control and the wire
   * disagree. When they do, this is the editor that is always right, because it edits exactly what
   * is sent. `Ctrl+Enter` applies, `Esc` cancels, and a document that does not parse is refused
   * before it reaches the game, so the game's error is never the first sign of a typo.
   */
  import { untrack } from 'svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Card from '$lib/components/shared/ui/Card.svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';

  let {
    value,
    applyLabel = 'Apply',
    onApply,
    onCancel,
    onDone,
  }: {
    value: unknown;
    applyLabel?: string;
    /** Resolves `true` when the game took it — the editor stays open on `false` so nothing is lost. */
    onApply: (parsed: unknown) => Promise<boolean> | boolean | void;
    onCancel: () => void;
    /** Called once the game took the value. Defaults to {@link onCancel}: closing the editor. */
    onDone?: () => void;
  } = $props();

  // The starting text; edited from here on, so deliberately not derived.
  let text = $state(untrack(() => JSON.stringify(value ?? null, null, 2)));
  let area = $state<HTMLTextAreaElement | undefined>();
  let busy = $state(false);

  const problem = $derived.by(() => {
    try {
      JSON.parse(text);
      return null;
    } catch (e) {
      return e instanceof Error ? e.message : String(e);
    }
  });

  $effect(() => { area?.focus(); });

  async function apply() {
    if (problem || busy) return;
    busy = true;
    try {
      const ok = await onApply(JSON.parse(text));
      if (ok !== false) (onDone ?? onCancel)();
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onCancel();
    }
  }
</script>

<div class="je" onkeydown={onKey} role="presentation">
  <Card variant="subtle" padding="sm">
    <Textarea bind:value={text} bind:element={area} minRows={3} maxRows={16} ariaLabel="Value as JSON" onsubmit={apply} />
  </Card>
  {#if problem}
    <Alert variant="error" compact text={problem} />
  {/if}
  <div class="je-actions">
    <Button size="xs" variant="ghost" onclick={onCancel}>Cancel</Button>
    <Button
      size="xs"
      variant="primary"
      disabled={!!problem}
      loading={busy}
      tooltip={{ content: applyLabel, shortcut: 'Ctrl+Enter' }}
      onclick={() => void apply()}
    >{applyLabel}</Button>
  </div>
</div>

<style>
  .je { display: flex; flex-direction: column; gap: 6px; }
  .je-actions { display: flex; justify-content: flex-end; gap: 6px; }
</style>
