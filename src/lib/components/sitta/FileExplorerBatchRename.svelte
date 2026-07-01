<script lang="ts">
  /**
   * FileExplorerBatchRename — batch / multi rename for the built-in explorer.
   *
   * Builds new names from the selection with: find → replace (literal),
   * optional case transform, and optional sequence numbering inserted at a
   * `#` placeholder (or appended). A live preview shows old → new, flags
   * collisions, and the footer commits via the parent (which calls
   * `fs_rename_many` — a collision-safe two-phase rename).
   */
  import { tick } from 'svelte';
  import Modal from '../shared/Modal.svelte';
  import ModalHeader from '../shared/ModalHeader.svelte';
  import ModalFooter from '../shared/ModalFooter.svelte';
  import Button from '../shared/ui/Button.svelte';
  import { ArrowRight, AlertCircle } from 'lucide-svelte';

  let {
    items,
    onCancel,
    onApply,
  }: {
    items: { name: string; path: string }[];
    onCancel: () => void;
    onApply: (pairs: { from: string; to: string }[]) => void;
  } = $props();

  let find       = $state('');
  let replace    = $state('');
  let caseMode   = $state<'none' | 'lower' | 'upper'>('none');
  let seqOn      = $state(false);
  let seqStart   = $state(1);
  let seqPad     = $state(2);
  let extToo     = $state(false); // apply find/replace to the extension too

  function splitName(name: string): [string, string] {
    const dot = name.lastIndexOf('.');
    return dot > 0 ? [name.slice(0, dot), name.slice(dot)] : [name, ''];
  }
  function pad(n: number): string { return String(n).padStart(Math.max(0, seqPad), '0'); }

  function transform(name: string, idx: number): string {
    let [stem, ext] = splitName(name);
    const applyTo = (s: string) => {
      let out = find ? s.split(find).join(replace) : s;
      if (caseMode === 'lower') out = out.toLowerCase();
      else if (caseMode === 'upper') out = out.toUpperCase();
      return out;
    };
    stem = applyTo(stem);
    if (extToo && ext) ext = applyTo(ext);
    if (seqOn) {
      const num = pad(seqStart + idx);
      stem = stem.includes('#') ? stem.split('#').join(num) : `${stem}${num}`;
    }
    return `${stem}${ext}`;
  }

  const rows = $derived(items.map((it, i) => {
    const next = transform(it.name, i).trim();
    return { from: it.path, oldName: it.name, newName: next };
  }));

  // Collisions: empty names, or two rows producing the same name.
  const dupes = $derived.by(() => {
    const seen = new Map<string, number>();
    for (const r of rows) seen.set(r.newName.toLowerCase(), (seen.get(r.newName.toLowerCase()) ?? 0) + 1);
    return seen;
  });
  function rowBad(name: string): boolean {
    return name === '' || (dupes.get(name.toLowerCase()) ?? 0) > 1;
  }
  const changed = $derived(rows.filter(r => r.newName !== r.oldName && r.newName !== ''));
  const hasError = $derived(rows.some(r => rowBad(r.newName)));
  const canApply = $derived(changed.length > 0 && !hasError);

  function parentDir(path: string): string {
    const i = Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/'));
    return i >= 0 ? path.slice(0, i + 1) : '';
  }

  function apply() {
    if (!canApply) return;
    const pairs = changed.map(r => ({ from: r.from, to: parentDir(r.from) + r.newName }));
    onApply(pairs);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); onCancel(); }
    else if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); apply(); }
  }

  let findEl = $state<HTMLInputElement | null>(null);
  $effect(() => { tick().then(() => findEl?.focus()); });
</script>

<svelte:window onkeydown={onKeydown} />

<Modal onClose={onCancel} width="640px" height="560px" padBody={false} ariaLabel="Rename items">
  {#snippet header()}
    <ModalHeader onClose={onCancel} title={`Rename ${items.length} items`} />
  {/snippet}

  <div class="br-body">
    <div class="br-controls">
      <div class="br-row">
        <label class="br-field">
          <span class="br-label">Find</span>
          <input bind:this={findEl} class="br-input" type="text" bind:value={find} placeholder="text to replace" spellcheck="false" autocomplete="off" />
        </label>
        <label class="br-field">
          <span class="br-label">Replace with</span>
          <input class="br-input" type="text" bind:value={replace} placeholder="(empty = delete)" spellcheck="false" autocomplete="off" />
        </label>
      </div>
      <div class="br-row br-row-opts">
        <label class="br-check"><input type="checkbox" bind:checked={extToo} /> Include extension</label>
        <label class="br-field br-field-sm">
          <span class="br-label">Case</span>
          <select class="br-input" bind:value={caseMode}>
            <option value="none">Keep</option>
            <option value="lower">lower</option>
            <option value="upper">UPPER</option>
          </select>
        </label>
      </div>
      <div class="br-row br-row-opts">
        <label class="br-check"><input type="checkbox" bind:checked={seqOn} /> Number items</label>
        {#if seqOn}
          <label class="br-field br-field-sm"><span class="br-label">Start</span><input class="br-input" type="number" min="0" bind:value={seqStart} /></label>
          <label class="br-field br-field-sm"><span class="br-label">Digits</span><input class="br-input" type="number" min="1" max="6" bind:value={seqPad} /></label>
          <span class="br-hint">Inserted at <code>#</code>, else appended</span>
        {/if}
      </div>
    </div>

    <div class="br-preview" role="list" aria-label="Rename preview">
      {#each rows as r (r.from)}
        <div class="br-prow" class:bad={rowBad(r.newName)} class:nochange={r.newName === r.oldName} role="listitem">
          <span class="br-old">{r.oldName}</span>
          <span class="br-arrow"><ArrowRight size={12} /></span>
          {#if rowBad(r.newName)}
            <span class="br-new br-new-bad"><AlertCircle size={12} /> {r.newName || '(empty)'}</span>
          {:else}
            <span class="br-new">{r.newName}</span>
          {/if}
        </div>
      {/each}
    </div>
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="br-foot">{changed.length} of {items.length} will change{hasError ? ' · fix name conflicts' : ''}</span>
      <div class="br-foot-btns">
        <Button variant="ghost" onclick={onCancel}>Cancel</Button>
        <Button variant="primary" disabled={!canApply} onclick={apply}>Rename {changed.length}</Button>
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .br-body { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .br-controls { padding: 14px 16px; display: flex; flex-direction: column; gap: 10px; border-bottom: 1px solid var(--border-subtle); }
  .br-row { display: flex; gap: 12px; align-items: flex-end; }
  .br-row-opts { align-items: center; flex-wrap: wrap; gap: 16px; }
  .br-field { display: flex; flex-direction: column; gap: 4px; flex: 1; min-width: 0; }
  .br-field-sm { flex: 0 0 auto; }
  .br-label { font-size: 11px; color: var(--text-muted); font-weight: 600; }
  .br-input { background: var(--bg-input); border: 1px solid var(--border-subtle); border-radius: var(--radius-sm); color: var(--text-primary); font-family: var(--font-ui-sans); font-size: 12.5px; padding: 5px 8px; outline: none; }
  .br-input:focus { border-color: var(--border-focus); box-shadow: 0 0 0 2px rgba(61,127,255,0.2); }
  .br-field-sm .br-input { width: 76px; }
  .br-check { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-secondary); cursor: pointer; }
  .br-hint { font-size: 11px; color: var(--text-disabled); }
  .br-hint code { font-family: var(--font-code); background: var(--bg-hover); padding: 0 4px; border-radius: 3px; }
  .br-preview { flex: 1; overflow-y: auto; padding: 8px 16px; display: flex; flex-direction: column; gap: 2px; scrollbar-width: thin; scrollbar-color: var(--scrollbar-thumb) transparent; }
  .br-prow { display: flex; align-items: center; gap: 8px; font-size: 12px; padding: 3px 0; min-width: 0; }
  .br-prow.nochange { opacity: 0.5; }
  .br-old { color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; text-align: right; }
  .br-arrow { color: var(--text-disabled); flex-shrink: 0; display: inline-flex; }
  .br-new { color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }
  .br-new-bad { color: var(--danger); display: inline-flex; align-items: center; gap: 4px; }
  .br-foot { font-size: 11.5px; color: var(--text-muted); }
  .br-foot-btns { display: flex; gap: 8px; }
</style>
