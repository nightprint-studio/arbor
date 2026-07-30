<script lang="ts">
  /**
   * Scale-aware chord-progression generator. Pick a key (root + scale mode) and a
   * progression of degrees (Roman numerals `I V vi IV` or numbers `1 5 6 4`); the
   * modal builds diatonic chords by stacking scale thirds as parallel degree lanes
   * (`n(0 4 5 3 & 2 6 7 5 & 4 8 9 7).scale("c:major")`), so the engine resolves the
   * right quality for each degree in any mode. Preview on the audition bus, insert
   * at the caret.
   *
   * Keyboard-first: the root field auto-focuses, Ctrl/Cmd+Enter inserts, Esc
   * cancels (Modal).
   */
  import { Music, Play } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { merulaEngine } from '../stores/engine.svelte';
  import { scalesStore } from '../stores/scales.svelte';

  let { onInsert, onClose, projectDir }:
    { onInsert: (text: string) => void; onClose: () => void; projectDir?: string } = $props();

  const modeOptions = $derived(scalesStore.modes.map((m) => ({ value: m.name, label: m.name })));
  const defaultMode = $derived(
    scalesStore.modes.some((m) => m.name === 'major') ? 'major' : (scalesStore.modes[0]?.name ?? 'major'),
  );

  let root = $state('c');
  let mode = $state('');
  let prog = $state('I V vi IV');
  let size = $state('3'); // '3' triad · '4' seventh

  // Seed the mode once the catalogue is in (it loads async). A plain flag (not
  // tracked) so the effect depends only on `defaultMode` — no self-trigger loop.
  let seeded = false;
  $effect(() => {
    const dm = defaultMode;
    if (!seeded && dm) { seeded = true; mode = dm; }
  });

  const ROMAN: Record<string, number> = { i: 1, ii: 2, iii: 3, iv: 4, v: 5, vi: 6, vii: 7 };
  const ROMANS = ['I', 'II', 'III', 'IV', 'V', 'VI', 'VII'];
  const romanOf = (d: number) => (d >= 0 && d < 7 ? ROMANS[d] : String(d + 1));

  /** Parse a progression into 0-based scale degrees (Roman or arabic). */
  const degrees = $derived.by(() =>
    prog
      .split(/[\s,–\-]+/)
      .map((t) => t.toLowerCase().replace(/[°+]/g, ''))
      .filter(Boolean)
      .map((t) => {
        if (ROMAN[t] != null) return ROMAN[t] - 1;
        const n = parseInt(t, 10);
        return Number.isFinite(n) && n >= 1 && n <= 14 ? n - 1 : -1;
      })
      .filter((d) => d >= 0),
  );

  const safeRoot = $derived(root.trim().toLowerCase());
  const sizeN = $derived(size === '4' ? 4 : 3);

  /** `n(<roots> & <thirds> & …).scale("root:mode")` — one lane per chord tone. */
  const expr = $derived.by(() => {
    if (!degrees.length || !safeRoot || !mode) return '';
    const lanes: string[] = [];
    for (let t = 0; t < sizeN; t++) lanes.push(degrees.map((d) => d + 2 * t).join(' '));
    return `n(${lanes.join(' & ')}).scale("${safeRoot}:${mode}")`;
  });

  const valid = $derived(expr.length > 0);

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

<Modal {onClose} width="560px" height="460px" ariaLabel="Chord progression generator">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Music size={14} />
      <span class="modal-title">Chord progression</span>
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="cp" onkeydown={onKeydown}>
    <div class="row">
      <div class="col root">
        <FormField label="Root">
          <Input bind:value={root} autofocus placeholder="c" ariaLabel="Scale root" />
        </FormField>
      </div>
      <div class="col">
        <FormField label="Scale">
          <Select value={mode} options={modeOptions} onchange={(v) => (mode = v)} />
        </FormField>
      </div>
      <div class="col">
        <FormField label="Chord">
          <Select value={size} options={[{ value: '3', label: 'Triad' }, { value: '4', label: 'Seventh' }]} onchange={(v) => (size = v)} />
        </FormField>
      </div>
    </div>

    <FormField label="Progression" hint="Roman numerals (I V vi IV) or numbers (1 5 6 4)">
      <Input bind:value={prog} placeholder="I V vi IV" ariaLabel="Progression" />
    </FormField>

    {#if degrees.length}
      <div class="chips">
        {#each degrees as d, i (i)}<span class="chip">{romanOf(d)}</span>{/each}
      </div>
    {/if}

    <div class="expr">
      <code>{expr || '—'}</code>
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
  .cp { display: flex; flex-direction: column; gap: 14px; padding: 16px 4px 4px; }
  .row { display: flex; gap: 16px; align-items: flex-end; }
  .col { flex: 1; min-width: 0; }
  .col.root { flex: 0 0 96px; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip {
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary);
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); padding: 2px 8px;
  }
  .expr { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .expr code {
    flex: 1; min-width: 0;
    font-family: var(--font-code); font-size: var(--font-size-sm); color: var(--text-primary);
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); padding: 4px 8px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
</style>
