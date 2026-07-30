<script lang="ts">
  /**
   * Euclidean rhythm generator. Pick a leaf (a sound like `bd` or a note like
   * `c4`), the number of hits + steps, and an optional rotation; the modal shows
   * the distribution as a step row, previews it on the audition bus, and inserts
   * the mini-notation expression (`s(bd(3,8))` / `n(c4(3,8,2))`) at the caret.
   *
   * Keyboard-first: the leaf field auto-focuses, Ctrl/Cmd+Enter inserts, Esc
   * cancels (Modal). The step preview is indicative — the engine resolves the
   * exact distribution via the language's `(n,k)` operator.
   */
  import { Grid3x3, Play } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { merulaEngine } from '../stores/engine.svelte';

  let { onInsert, onClose, projectDir }:
    { onInsert: (text: string) => void; onClose: () => void; projectDir?: string } = $props();

  let leaf  = $state('bd');
  let hits  = $state(3);
  let steps = $state(8);
  let rot   = $state(0);

  const NOTE_RE = /^[a-gA-G][sf]?-?\d+$/;
  const isNote = $derived(NOTE_RE.test(leaf.trim()));
  const safeLeaf = $derived(leaf.trim());
  const clampedHits = $derived(Math.max(0, Math.min(hits, Math.max(1, steps))));

  /** Canonical mini-notation, e.g. `s(bd(3,8))` or `n(c4(3,8,2))`. */
  const expr = $derived.by(() => {
    const inner = rot > 0
      ? `${safeLeaf}(${clampedHits},${steps},${rot})`
      : `${safeLeaf}(${clampedHits},${steps})`;
    return `${isNote ? 'n' : 's'}(${inner})`;
  });

  const valid = $derived(safeLeaf.length > 0 && steps >= 1);

  /** Standard "spread" euclidean (Bresenham bucket), rotated — for the visual. */
  const pattern = $derived.by(() => {
    const n = Math.max(1, steps);
    const k = Math.max(0, Math.min(clampedHits, n));
    const out: boolean[] = [];
    let bucket = 0;
    for (let i = 0; i < n; i++) {
      bucket += k;
      if (bucket >= n) { bucket -= n; out.push(true); } else out.push(false);
    }
    const r = ((rot % n) + n) % n;
    return out.slice(r).concat(out.slice(0, r));
  });

  function preview() {
    if (!valid) return;
    void merulaEngine.playSnippet(expr, projectDir);
  }
  function insert() {
    if (!valid) return;
    onInsert(expr);
    onClose();
  }
  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); insert(); }
  }
</script>

<Modal {onClose} width="560px" height="440px" ariaLabel="Euclidean rhythm generator">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Grid3x3 size={14} />
      <span class="modal-title">Euclidean rhythm</span>
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="eg" onkeydown={onKeydown}>
    <FormField label="Sound or note">
      <Input bind:value={leaf} autofocus placeholder="bd" ariaLabel="Sound or note leaf" />
    </FormField>

    <div class="row">
      <FormField label="Hits">
        <NumberStepper value={hits} min={0} max={steps} step={1} narrow ariaLabel="Hits" onchange={(v) => (hits = v)} />
      </FormField>
      <FormField label="Steps">
        <NumberStepper value={steps} min={1} step={1} narrow ariaLabel="Steps" onchange={(v) => (steps = v)} />
      </FormField>
      <FormField label="Rotation">
        <NumberStepper value={rot} min={0} max={Math.max(0, steps - 1)} step={1} narrow ariaLabel="Rotation" onchange={(v) => (rot = v)} />
      </FormField>
    </div>

    <div class="steps" role="img" aria-label={`${clampedHits} hits over ${steps} steps`}>
      {#each pattern as hit, i (i)}
        <span class="step" class:on={hit}></span>
      {/each}
    </div>

    <div class="expr">
      <code>{expr}</code>
      <Button variant="ghost" onclick={preview} disabled={!valid} tooltip={{ content: 'Preview on the audition bus' }}>
        {#snippet iconStart()}<Play size={13} />{/snippet}
        Preview
      </Button>
    </div>
  </div>

  {#snippet footer()}
    <ModalFooter>
      <Button variant="ghost" onclick={onClose}>Cancel</Button>
      <Button variant="primary" disabled={!valid} onclick={insert}
              tooltip={{ content: 'Insert at the caret', shortcut: 'Ctrl+Enter' }}>
        Insert
      </Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .eg { display: flex; flex-direction: column; gap: 14px; padding: 16px 4px 4px; }
  .row { display: flex; gap: 16px; }
  .steps {
    display: flex; flex-wrap: wrap; gap: 4px;
    padding: 12px; border-radius: var(--radius-md);
    background: var(--bg-elevated); border: 1px solid var(--border-subtle);
  }
  .step {
    width: 16px; height: 16px; border-radius: 4px;
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
  }
  .step.on { background: var(--accent); border-color: var(--accent); }
  .expr {
    display: flex; align-items: center; justify-content: space-between; gap: 10px;
  }
  .expr code {
    font-family: var(--font-code); font-size: var(--font-size-sm); color: var(--text-primary);
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); padding: 4px 8px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
</style>
