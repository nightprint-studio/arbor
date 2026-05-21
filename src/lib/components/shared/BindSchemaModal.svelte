<script lang="ts">
  /**
   * BindSchemaModal — bind a studio file (RON, JSON, TOML) to a schema
   * source. Writes a per-file override to `.arbor/studio.toml` (via
   * the StudioPanel's caller) so the next time the file is opened in
   * Studio the schema lights up automatically — same machinery as an
   * inline `//! ron-studio:` directive (RON), just managed centrally.
   *
   * Per-format schema sources:
   *   · `ron`  → Rust crate (`.rs`)
   *   · `json` → JSON Schema (`*.schema.json`, any `.json` with
   *              `$schema` works too)
   *   · `toml` → either a Rust crate (`.rs`) OR a JSON Schema; the
   *              backend dispatches on the file extension.
   *
   * Flow: pick the schema source → host probes the file for root
   * candidates → user picks one → Save fires `onSave(rsFile, rootCanonical)`.
   */

  import { untrack } from 'svelte';
  import Modal from './Modal.svelte';
  import FilePickerModal from './FilePickerModal.svelte';
  import Spinner from './ui/Spinner.svelte';
  import { FolderOpen, FileCode, Link2, AlertCircle, Check, X, Plus, Info } from 'lucide-svelte';
  import { studioBackend, type RootCandidate, type StudioFormat } from '$lib/ipc/studio-format';

  import { tooltip } from '$lib/actions/tooltip';

  interface InitialSchema {
    rs_file:           string;
    root_type:         string;
    reference_fields?: string[];
  }

  let {
    formatId = 'ron',
    relativePath,
    fileName,
    targetKind = 'file',
    initial,
    onSave,
    onClose,
  }: {
    /** Which studio format is this binding for. Drives picker
     *  extensions + the backend used for `schemaProbe`. */
    formatId?:        StudioFormat;
    /** Pattern recorded as `[[overrides]].glob` — either an exact
     *  repo-relative file path or a `<folder>/**` glob. */
    relativePath:     string;
    /** Display label in the header (filename or `<folder>/`). */
    fileName:         string;
    /** Affects header copy + help text; the underlying write is the same. */
    targetKind?:      'file' | 'folder';
    initial:          InitialSchema | null;
    /** Save callback. `referenceFields = null` means "leave the existing
     *  list untouched"; an array (possibly empty) replaces it. */
    onSave:           (rsFile: string, rootType: string, referenceFields: string[] | null) => void;
    onClose:          () => void;
  } = $props();

  const BE = $derived(studioBackend(formatId));

  /** Per-format picker config — the file extensions to allow + the
   *  human label shown above the input. */
  const pickerConfig = $derived.by(() => {
    switch (formatId) {
      case 'ron':
        return {
          extensions: ['rs'],
          label:      'Rust source (.rs)',
          placeholder: '/abs/path/to/types.rs',
          pickerTitle: 'Pick Rust source for schema',
          probeHint:   'Probing crate…',
        };
      case 'json':
        return {
          extensions: ['json', 'schema.json'],
          label:      'JSON Schema (.json / .schema.json)',
          placeholder: '/abs/path/to/types.schema.json',
          pickerTitle: 'Pick JSON Schema for binding',
          probeHint:   'Probing schema…',
        };
      case 'toml':
        return {
          extensions: ['rs', 'json', 'schema.json'],
          label:      'Schema source (.rs or .schema.json)',
          placeholder: '/abs/path/to/types.rs or types.schema.json',
          pickerTitle: 'Pick schema source for binding',
          probeHint:   'Probing schema source…',
        };
      default:
        return {
          extensions: ['rs', 'json', 'schema.json'],
          label:      'Schema source',
          placeholder: '/abs/path/to/schema',
          pickerTitle: 'Pick schema source',
          probeHint:   'Probing schema…',
        };
    }
  });

  let rsFile        = $state<string>(untrack(() => initial?.rs_file ?? ''));
  let rootType      = $state<string>(untrack(() => initial?.root_type ?? ''));
  let candidates    = $state<RootCandidate[]>([]);
  let probing       = $state(false);
  let probeError    = $state<string | null>(null);
  let crateName     = $state<string | null>(null);
  let pickerOpen    = $state(false);

  // ── Reference-field patterns ──────────────────────────────────────────
  // Optional list that overrides the built-in convention (`*_id`,
  // `*_ref`, `target/source/...`). Empty = inherit convention. We track
  // an extra `refsDirty` flag so re-binding via the UI doesn't reset a
  // user's hand-curated list unless they actually touched it here.
  let refs       = $state<string[]>(untrack(() => initial?.reference_fields ? [...initial.reference_fields] : []));
  let refsDirty  = $state(false);
  let refsBuffer = $state('');

  function addRef(raw: string) {
    const v = raw.trim();
    if (!v) return;
    if (refs.includes(v)) { refsBuffer = ''; return; }
    refs = [...refs, v];
    refsDirty = true;
    refsBuffer = '';
  }
  function removeRef(i: number) {
    refs = refs.filter((_, ix) => ix !== i);
    refsDirty = true;
  }
  function clearRefs() {
    refs = [];
    refsDirty = true;
  }
  function onRefKey(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ',' || e.key === ' ') {
      e.preventDefault();
      addRef(refsBuffer);
    } else if (e.key === 'Backspace' && refsBuffer === '' && refs.length > 0) {
      removeRef(refs.length - 1);
    }
  }

  // Probe whenever rsFile changes — debounce-light: only fires for
  // paths that look like the picker's allowed extensions. Errors don't
  // dismiss the existing rootType; the user can still save against the
  // previous probe's result.
  function pathLooksProbeable(file: string): boolean {
    const lc = file.toLowerCase();
    return pickerConfig.extensions.some(ext => lc.endsWith('.' + ext))
      || lc.endsWith('.schema.json');
  }

  $effect(() => {
    const file = rsFile;
    if (!file) {
      candidates = []; crateName = null; probeError = null; return;
    }
    if (!pathLooksProbeable(file)) return;
    probing = true; probeError = null;
    BE.schemaProbe(file)
      .then((probe) => {
        candidates = probe.root_candidates ?? [];
        crateName  = probe.crate_name ?? null;
        // Only auto-fill rootType when the modal opened without an
        // initial binding — otherwise respect the user's existing pick.
        if (!rootType && candidates.length === 1) {
          rootType = candidates[0].canonical_path;
        }
      })
      .catch((e) => { probeError = String(e); candidates = []; crateName = null; })
      .finally(() => { probing = false; });
  });

  const canSave = $derived(rsFile.trim().length > 0 && rootType.trim().length > 0);

  function save() {
    if (!canSave) return;
    // If the user hasn't interacted with the chip editor, send `null`
    // so the host preserves whatever was already on disk. Otherwise
    // send the current array (empty array means "explicitly clear back
    // to the built-in convention").
    onSave(rsFile, rootType, refsDirty ? refs : null);
  }

  function onPickerConfirm(p: string) {
    pickerOpen = false;
    rsFile = p;
    rootType = '';  // force a re-pick when the .rs source changes
  }
</script>

<Modal onClose={onClose} width="min(560px, 92vw)" ariaLabel="Bind schema">
  {#snippet header()}
    <span class="bs-icon-wrap"><Link2 size={16} /></span>
    <span class="bs-title">{targetKind === 'folder' ? 'Bind schema to folder' : 'Bind schema'}</span>
    <span class="bs-target" use:tooltip={relativePath}>{fileName}</span>
    <div class="bs-spacer"></div>
    <button class="mac-close-btn" onclick={onClose} aria-label="Close"></button>
  {/snippet}

  <div class="bs-body">
    <p class="bs-help">
      {#if targetKind === 'folder'}
        The binding is stored in <code>.arbor/studio.toml</code> as an
        <code>[[overrides]]</code> entry whose glob matches every file
        under this folder. Per-file overrides still win when present.
      {:else}
        The schema binding is stored in <code>.arbor/studio.toml</code>
        as a per-file <code>[[overrides]]</code> entry. Open this file
        again in Studio and the schema will load automatically.
      {/if}
    </p>

    <!-- ── Step 1: pick the schema source ──────────────────────────── -->
    <label class="bs-field">
      <span class="bs-label">{pickerConfig.label}</span>
      <div class="bs-input-row">
        <input class="bs-input"
               type="text"
               bind:value={rsFile}
               placeholder={pickerConfig.placeholder}
               spellcheck="false" />
        <button class="bs-icon-btn"
                onclick={() => pickerOpen = true}
                use:tooltip={'Browse…'}
                aria-label="Browse">
          <FolderOpen size={14} />
        </button>
      </div>
      {#if probing}
        <span class="bs-hint"><Spinner size="sm" /> {pickerConfig.probeHint}</span>
      {:else if probeError}
        <span class="bs-hint bs-hint-err"><AlertCircle size={11} /> {probeError}</span>
      {:else if crateName}
        <span class="bs-hint bs-hint-ok"><Check size={11} /> {formatId === 'json' ? 'Schema' : 'Crate'}: <code>{crateName}</code> · {candidates.length} root candidate{candidates.length === 1 ? '' : 's'}</span>
      {/if}
    </label>

    <!-- ── Step 2: pick a root type ────────────────────────────────── -->
    <label class="bs-field">
      <span class="bs-label">Root type</span>
      {#if candidates.length === 0 && !rsFile}
        <span class="bs-hint">Pick a schema source first.</span>
      {:else if candidates.length === 0 && !probing}
        <input class="bs-input"
               type="text"
               bind:value={rootType}
               placeholder="e.g. crate::config::WorldConfig"
               spellcheck="false" />
        <span class="bs-hint">No public structs/enums detected — enter a canonical path manually.</span>
      {:else}
        <select class="bs-input bs-select" bind:value={rootType} disabled={probing}>
          <option value="" disabled>— choose —</option>
          {#each candidates as c (c.canonical_path)}
            <option value={c.canonical_path}>
              {c.name} · {c.kind}
            </option>
          {/each}
        </select>
        {#if rootType}
          <span class="bs-hint"><code>{rootType}</code></span>
        {/if}
      {/if}
    </label>

    <!-- ── Step 3: optional reference-field patterns ─────────────── -->
    <div class="bs-field">
      <span class="bs-label">
        Reference fields
        <span class="bs-label-opt">— optional</span>
      </span>
      <div class="bs-chips" role="list">
        {#each refs as p, i (p + ':' + i)}
          <span class="bs-chip" role="listitem">
            <span class="bs-chip-text">{p}</span>
            <button class="bs-chip-x" onclick={() => removeRef(i)} aria-label="Remove {p}">
              <X size={9} />
            </button>
          </span>
        {/each}
        <input class="bs-chip-input"
               type="text"
               bind:value={refsBuffer}
               onkeydown={onRefKey}
               placeholder={refs.length === 0 ? 'e.g. target, *_id, enemy_handle' : ''}
               spellcheck="false" />
        <button class="bs-icon-btn bs-chip-add"
                onclick={() => addRef(refsBuffer)}
                use:tooltip={'Add pattern'}
                aria-label="Add">
          <Plus size={12} />
        </button>
      </div>
      <span class="bs-hint">
        <Info size={11} />
        <span>
          Replaces the built-in convention (<code>*_id</code>, <code>*_ref</code>,
          <code>target</code>, <code>source</code>, …) for files matched by this binding.
          Supports <code>*suffix</code> / <code>prefix*</code> / exact names.
          Leave empty to inherit the convention.
        </span>
      </span>
      {#if refs.length > 0}
        <button class="bs-btn bs-btn-link" onclick={clearRefs}>Clear list</button>
      {/if}
    </div>

    <div class="bs-actions">
      <button class="bs-btn bs-btn-secondary" onclick={onClose}>Cancel</button>
      <button class="bs-btn bs-btn-primary" disabled={!canSave} onclick={save}>
        <FileCode size={12} /> Save binding
      </button>
    </div>
  </div>
</Modal>

{#if pickerOpen}
  <FilePickerModal
    mode="file"
    title={pickerConfig.pickerTitle}
    extensions={pickerConfig.extensions}
    initialPath={rsFile || undefined}
    onConfirm={onPickerConfirm}
    onCancel={() => pickerOpen = false}
  />
{/if}

<style>
  .bs-icon-wrap {
    display: inline-flex; align-items: center; justify-content: center;
    color: var(--accent);
    flex-shrink: 0;
  }
  .bs-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .bs-target {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-secondary);
    background: var(--bg-overlay);
    padding: 2px 8px;
    border-radius: 8px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    max-width: 280px;
  }
  .bs-spacer { flex: 1; }

  .bs-body {
    display: flex; flex-direction: column;
    gap: 14px;
    padding: 4px 4px 6px;
  }
  .bs-help {
    margin: 0;
    font-size: 11.5px;
    color: var(--text-secondary);
    line-height: 1.5;
  }
  .bs-help code {
    font-family: var(--font-code);
    font-size: 11px;
    padding: 0 3px;
    background: var(--bg-overlay);
    border-radius: 3px;
  }

  .bs-field {
    display: flex; flex-direction: column;
    gap: 5px;
  }
  .bs-label {
    font-size: 11px;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.4px;
    font-weight: 600;
  }
  .bs-input-row { display: flex; gap: 6px; }
  .bs-input {
    flex: 1;
    background: var(--bg-base);
    color: var(--text-primary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 6px 8px;
    font-size: 12px;
    font-family: var(--font-code);
    outline: none;
    transition: border-color var(--transition-fast);
  }
  .bs-input:focus { border-color: var(--accent); }
  .bs-select { font-family: var(--font-ui-sans); }
  .bs-icon-btn {
    display: inline-flex; align-items: center; justify-content: center;
    width: 30px; height: 30px;
    background: var(--bg-overlay);
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .bs-icon-btn:hover { background: var(--bg-hover); color: var(--text-primary); border-color: var(--accent); }

  .bs-hint {
    display: inline-flex; align-items: center; gap: 4px;
    font-size: 11px;
    color: var(--text-muted);
  }
  .bs-hint code {
    font-family: var(--font-code);
    color: var(--text-secondary);
    background: var(--bg-overlay);
    padding: 0 4px;
    border-radius: 3px;
  }
  .bs-hint-ok  { color: var(--success, #98c379); }
  .bs-hint-err { color: var(--error, #e06c75); }

  /* ── Reference-field chip editor ──────────────────────────────────── */
  .bs-label-opt {
    margin-left: 6px;
    text-transform: none;
    letter-spacing: 0;
    color: var(--text-muted);
    font-weight: 400;
    font-size: 10.5px;
  }
  .bs-chips {
    display: flex; flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    padding: 4px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    min-height: 32px;
  }
  .bs-chips:focus-within { border-color: var(--accent); }
  .bs-chip {
    display: inline-flex; align-items: center;
    gap: 4px;
    padding: 2px 4px 2px 8px;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
    border-radius: 10px;
    font-family: var(--font-code);
    font-size: 11px;
    line-height: 1;
  }
  .bs-chip-text { line-height: 1; }
  .bs-chip-x {
    display: inline-flex; align-items: center; justify-content: center;
    width: 14px; height: 14px;
    background: transparent;
    color: inherit;
    border: none;
    border-radius: 50%;
    cursor: pointer;
    padding: 0;
  }
  .bs-chip-x:hover { background: rgba(0,0,0,0.18); }
  .bs-chip-input {
    flex: 1;
    min-width: 80px;
    background: transparent;
    border: none;
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 12px;
    outline: none;
    padding: 2px 4px;
  }
  .bs-chip-input::placeholder { color: var(--text-disabled); }
  .bs-chip-add {
    width: 22px; height: 22px;
  }
  .bs-btn-link {
    align-self: flex-start;
    background: transparent;
    border: none;
    color: var(--accent);
    padding: 2px 0;
    font-size: 11px;
    cursor: pointer;
    text-decoration: underline;
  }
  .bs-btn-link:hover { color: var(--accent-hover, var(--accent)); }

  .bs-actions {
    display: flex; justify-content: flex-end;
    gap: 8px;
    margin-top: 6px;
  }
  .bs-btn {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 6px 14px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .bs-btn-secondary {
    background: var(--bg-overlay);
    color: var(--text-secondary);
  }
  .bs-btn-secondary:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .bs-btn-primary {
    background: var(--accent);
    color: #fff;
    border-color: var(--accent);
  }
  .bs-btn-primary:hover:not(:disabled) { filter: brightness(1.1); }
  .bs-btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
