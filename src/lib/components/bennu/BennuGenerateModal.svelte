<script lang="ts">
  /**
   * BennuGenerateModal — the "Generate" dialog (constructor · getters · setters ·
   * getters+setters · with) for the Java editor, à la IntelliJ's Alt+Insert.
   *
   * The generation is real (deterministic client-side string building via
   * `java-generate.ts`), and the result is handed back through `onInsert(text)`.
   * Accessor detection (`java-accessors.ts`) surfaces G/S/W chips per field so the
   * user sees what already exists; the constructor offers all-args / no-args /
   * required-args (final · @NonNull) / selected variants; naming (camel/snake),
   * classic-vs-fluent accessors, and the Java Style flags (final params, member
   * spacing) all flow into the preview live. What is MOCK today:
   *   • the field list falls back to a hardcoded trio when the active file has no
   *     detectable fields — the real, typed field list will come from the backend
   *     symbol model (see "Field source" note below);
   *   • the sticky option defaults live in an in-memory rune (`generateStore`),
   *     not yet on the filesystem (see that store's SEAM note).
   *
   * This component does NOT touch the editor directly — the caller wires
   * `onInsert` to the editor's imperative insert-at-caret API in the Wire phase
   * (mirrors merula's `insertAtCursor`).
   *
   * Keyboard-first: Mode segmented control is first-focused; Tab walks
   * mode → select-all → fields → options; Esc cancels (Modal owns it);
   * Ctrl/Cmd+Enter inserts.
   *
   * Reuses only shared widgets (Modal/Header/Footer, RadioGroup, Button, the
   * shared read-only CodeEditor) + bennu-local outline/generator/store.
   */
  import { Wand2, Check } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { CodeEditor } from '$lib/components/shared/ui/code-editor';
  import { tooltip } from '$lib/actions/tooltip';
  import { javaLanguage } from './java-lang';
  import { javaOutline, requiredFieldNames } from './java-outline';
  import { detectAccessors, flagsFor } from './java-accessors';
  import {
    generateMembers,
    type GenerateMode,
    type JavaField,
    type NamingStyle,
    type ConstructorVariant,
    type GenerateOptions,
    type JavaStyle,
  } from './java-generate';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { generateStore } from '$lib/stores/bennu/generate.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';

  let {
    mode: initialMode = 'getters-setters',
    onClose,
    onInsert,
  }: {
    /** Preselected generation mode (defaults to getters+setters). */
    mode?: GenerateMode;
    onClose: () => void;
    /** Called with the generated Java text when the user confirms. The Wire phase
     *  connects this to the editor's insert-at-caret imperative API. */
    onInsert: (text: string) => void;
  } = $props();

  // ── Mode ──────────────────────────────────────────────────────────────────────
  let mode = $state<GenerateMode>(initialMode);
  const MODE_OPTIONS = [
    { value: 'constructor',     label: 'Constructor' },
    { value: 'getters',         label: 'Getters' },
    { value: 'setters',         label: 'Setters' },
    { value: 'getters-setters', label: 'Getters + Setters' },
    { value: 'with',            label: 'With' },
  ];
  const showConstructorOpts = $derived(mode === 'constructor');
  // Naming applies to every accessor mode; the fluent/plain choice only affects
  // getters/setters (withers are always builder-style).
  const showAccessorOpts = $derived(
    mode === 'getters' || mode === 'setters' || mode === 'getters-setters',
  );

  // ── Field source ────────────────────────────────────────────────────────────
  // Derived from the active file's regex outline (`java-outline.ts`), keeping only
  // `field` symbols; `detail` carries the declared type. When the outline finds no
  // fields (empty file / demo-less session), we MOCK a canonical trio so the modal
  // is always demonstrable.
  //
  // NOTE — the REAL, reliably-typed field list will come from the backend symbol
  // model (the `bennu_symbols` / language-service seam that `java-outline.ts`'s
  // header already calls out). The regex outline is best-effort: it misses fields
  // whose modifiers/format it doesn't match and can't resolve inherited fields.
  // When the BE model lands, feed `JavaField[]` from it here — the rest of the
  // modal is agnostic to the source.

  // MOCK — fallback fields when the active file exposes none via the outline.
  const MOCK_FIELDS: JavaField[] = [
    { name: 'name',   type: 'String', required: true },
    { name: 'age',    type: 'int' },
    { name: 'email',  type: 'String' },
  ];

  const activeSource = $derived(projectStore.activeSource);
  const outlineSymbols = $derived(javaOutline(activeSource));
  // Fields declared `final` / `@NonNull` — the required-args constructor's scope.
  const requiredNames = $derived(requiredFieldNames(activeSource));

  /** The enclosing class name (first `class`/`enum`/`interface` symbol), or a
   *  MOCK placeholder when none is detectable. Used for the constructor name and
   *  fluent setter return type. */
  const className = $derived(
    outlineSymbols.find((s) => s.kind === 'class' || s.kind === 'enum')?.name
      // MOCK — no type declaration found; placeholder keeps the preview coherent.
      ?? 'Example',
  );

  const derivedFields = $derived<JavaField[]>(
    outlineSymbols
      .filter((s) => s.kind === 'field')
      .map((s) => ({
        name: s.name,
        type: (s.detail ?? 'Object').trim() || 'Object',
        required: requiredNames.has(s.name),
      })),
  );

  /** True when we fell back to the mock trio (surfaced in the UI as a hint). */
  const usingMockFields = $derived(derivedFields.length === 0);
  const fields = $derived<JavaField[]>(usingMockFields ? MOCK_FIELDS : derivedFields);

  // Which fields already have a getter / setter / with in the active file —
  // surfaced as G/S/W chips on each row so the user avoids regenerating existing
  // accessors. Pure detection lives in `java-accessors.ts`.
  const accessorMap = $derived(detectAccessors(activeSource, fields.map((f) => f.name)));

  // ── Field selection ───────────────────────────────────────────────────────────
  // Selection is a Set of field names. Seed it to "all selected" and keep it in
  // sync when the underlying field list changes (file switch / mock↔real).
  let selected = $state<Set<string>>(new Set());
  let lastFieldsKey = '';
  $effect(() => {
    const key = fields.map((f) => f.name).join('|');
    if (key !== lastFieldsKey) {
      lastFieldsKey = key;
      selected = new Set(fields.map((f) => f.name)); // default: all included
    }
  });

  const allSelected = $derived(fields.length > 0 && fields.every((f) => selected.has(f.name)));
  const noneSelected = $derived(selected.size === 0);

  function toggleField(name: string) {
    const next = new Set(selected);
    if (next.has(name)) next.delete(name); else next.add(name);
    selected = next;
  }
  function toggleAll() {
    selected = allSelected ? new Set() : new Set(fields.map((f) => f.name));
  }

  // ── Options (sticky via the in-memory seam store) ─────────────────────────────
  let fluent = $state(generateStore.fluent);
  let naming = $state<NamingStyle>(generateStore.naming);
  let constructorVariant = $state<ConstructorVariant>(generateStore.constructorVariant);
  // Mirror changes back into the store so they stick for the next open (MOCK
  // persistence — see generateStore SEAM note).
  $effect(() => generateStore.setFluent(fluent));
  $effect(() => generateStore.setNaming(naming));
  $effect(() => generateStore.setConstructorVariant(constructorVariant));

  const FLUENT_OPTIONS = [
    { value: 'plain',  label: 'Classic', description: 'get/set · void' },
    { value: 'fluent', label: 'Fluent',  description: 'record-style · this' },
  ];
  const NAMING_OPTIONS = [
    { value: 'camelCase',  label: 'camelCase' },
    { value: 'snake_case', label: 'snake_case' },
  ];
  const CTOR_VARIANT_OPTIONS = [
    { value: 'all',      label: 'All-args' },
    { value: 'none',     label: 'No-args' },
    { value: 'required', label: 'Required' },
    { value: 'selected', label: 'Selected' },
  ];

  const requiredFields = $derived(fields.filter((f) => f.required));
  /** The field checklist only drives output for accessor/with modes and the
   *  "Selected" constructor variant; for all/none/required it's informational. */
  const checklistActive = $derived(
    mode !== 'constructor' || constructorVariant === 'selected',
  );
  /** True when "Required" is chosen but no field is final/@NonNull — degrades to a note. */
  const noRequiredFields = $derived(
    mode === 'constructor' && constructorVariant === 'required' && requiredFields.length === 0,
  );

  // ── Java Style (from the settings store) ──────────────────────────────────────
  const style = $derived<JavaStyle>({
    finalParams: bennuSettingsStore.finalParams,
    spaceInBraces: bennuSettingsStore.spaceInBraces,
    blankLineBetweenMembers: bennuSettingsStore.blankLineBetweenMembers,
  });

  // ── Generation ────────────────────────────────────────────────────────────────
  /** The fields the generated members actually cover. The constructor honours its
   *  variant (all / none / required / selected); accessor modes always use the
   *  checklist selection. */
  const targetFields = $derived.by<JavaField[]>(() => {
    if (mode === 'constructor') {
      switch (constructorVariant) {
        case 'all':      return fields;
        case 'none':     return [];
        case 'required': return requiredFields;
        case 'selected': return fields.filter((f) => selected.has(f.name));
      }
    }
    return fields.filter((f) => selected.has(f.name));
  });

  const options = $derived<GenerateOptions>({
    className,
    fluent,
    naming,
    style,
  });

  const preview = $derived(generateMembers(mode, targetFields, options));
  // A no-args constructor is legitimately empty of fields but still generates text.
  const canInsert = $derived(preview.trim().length > 0);

  function insert() {
    if (!canInsert) return;
    onInsert(preview);
    onClose();
  }

  // ── Keyboard: Ctrl/Cmd+Enter submits ──────────────────────────────────────────
  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      insert();
    }
  }
</script>

<Modal {onClose} width="720px" height="560px" padBody={false} bodyBorder ariaLabel="Generate Java members">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Wand2 size={14} />
      <span class="modal-title">Generate</span>
      <span class="gen-target">in <code>{className}</code></span>
    </ModalHeader>
  {/snippet}

  <div class="gen" onkeydown={onKey} role="presentation">
    <!-- Left column: choices -->
    <div class="gen-left">
      <section class="gen-sec">
        <span class="gen-label">Mode</span>
        <RadioGroup
          value={mode}
          options={MODE_OPTIONS}
          appearance="segment"
          size="sm"
          block
          onchange={(v) => (mode = v as GenerateMode)}
        />
      </section>

      <section class="gen-sec gen-fields">
        <div class="gen-fields-head">
          <span class="gen-label">Fields</span>
          <button
            type="button"
            class="gen-selall"
            onclick={toggleAll}
            disabled={fields.length === 0 || !checklistActive}
          >
            {allSelected ? 'Deselect all' : 'Select all'}
          </button>
        </div>

        {#if usingMockFields}
          <!-- MOCK — the active file exposed no fields via the outline. -->
          <p class="gen-mock-note">No fields detected in the active file — showing example fields.</p>
        {:else if !checklistActive}
          <p class="gen-mock-note">Field selection applies to the <strong>Selected</strong> constructor variant.</p>
        {/if}

        <div class="gen-field-list" class:muted={!checklistActive} role="group" aria-label="Class fields">
          {#each fields as f (f.name)}
            {@const acc = flagsFor(accessorMap, f.name)}
            <label class="gen-field" class:on={selected.has(f.name)}>
              <input
                type="checkbox"
                checked={selected.has(f.name)}
                onchange={() => toggleField(f.name)}
              />
              <span class="gen-field-check" aria-hidden="true">
                {#if selected.has(f.name)}<Check size={11} />{/if}
              </span>
              <span class="gen-field-name">{f.name}</span>
              {#if f.required}
                <span class="gen-field-req" use:tooltip={'final / @NonNull — required-args'}>req</span>
              {/if}
              <span class="gen-acc" aria-hidden="true">
                <span class="gen-acc-chip" class:on={acc.getter} use:tooltip={acc.getter ? 'Getter exists' : 'No getter'}>G</span>
                <span class="gen-acc-chip" class:on={acc.setter} use:tooltip={acc.setter ? 'Setter exists' : 'No setter'}>S</span>
                <span class="gen-acc-chip" class:on={acc.wither} use:tooltip={acc.wither ? 'With-method exists' : 'No with-method'}>W</span>
              </span>
              <span class="gen-field-type">{f.type}</span>
            </label>
          {/each}
        </div>
      </section>

      <section class="gen-sec gen-opts">
        <span class="gen-label">Options</span>

        {#if showConstructorOpts}
          <div class="gen-opt-row">
            <span class="gen-opt-name">Variant</span>
            <RadioGroup
              value={constructorVariant}
              options={CTOR_VARIANT_OPTIONS}
              appearance="segment"
              size="sm"
              onchange={(v) => (constructorVariant = v as ConstructorVariant)}
            />
          </div>
          {#if noRequiredFields}
            <p class="gen-inline-note">No <code>final</code> / <code>@NonNull</code> fields — required-args is a no-args constructor.</p>
          {/if}
        {/if}

        {#if showAccessorOpts}
          <div class="gen-opt-row">
            <span class="gen-opt-name">Style</span>
            <RadioGroup
              value={fluent ? 'fluent' : 'plain'}
              options={FLUENT_OPTIONS}
              appearance="segment"
              size="sm"
              onchange={(v) => (fluent = v === 'fluent')}
            />
          </div>
        {/if}

        <div class="gen-opt-row">
          <span class="gen-opt-name">Naming</span>
          <RadioGroup
            value={naming}
            options={NAMING_OPTIONS}
            appearance="segment"
            size="sm"
            onchange={(v) => (naming = v as NamingStyle)}
          />
        </div>
      </section>
    </div>

    <!-- Right column: live preview -->
    <div class="gen-right">
      <span class="gen-label gen-preview-label">Preview</span>
      <div class="gen-preview">
        {#if canInsert}
          <!-- One editor, updated via its `value` prop — NOT re-`{#key}`ed on every
               preview change, which would destroy + rebuild a full tree-sitter editor
               (grammar + parser + all extensions) on each option toggle and froze the
               modal. The read-only editor still reflects each new `preview` live. -->
          <CodeEditor value={preview} language={javaLanguage} readOnly />
        {:else}
          <div class="gen-preview-empty">
            <EmptyState message={noneSelected ? 'Select at least one field.' : 'Nothing to generate.'} />
          </div>
        {/if}
      </div>
    </div>
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="gen-hint">Inserts at the caret.</span>
      <div class="gen-actions">
        <Button variant="ghost" size="sm" onclick={onClose}>Cancel</Button>
        <Button
          variant="primary"
          size="sm"
          disabled={!canInsert}
          tooltip={{ content: 'Insert', shortcut: 'Ctrl+Enter' }}
          onclick={insert}
        >
          Insert
        </Button>
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .gen {
    display: grid;
    grid-template-columns: minmax(0, 320px) minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  /* ── Left column ── */
  .gen-left {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 14px;
    overflow-y: auto;
    border-right: 1px solid var(--border-subtle);
    min-height: 0;
  }

  .gen-sec { display: flex; flex-direction: column; gap: 7px; }
  .gen-fields { flex: 1; min-height: 0; }

  .gen-label {
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .gen-fields-head { display: flex; align-items: center; justify-content: space-between; }
  .gen-selall {
    background: transparent; border: none; padding: 0;
    font-size: 11px; color: var(--accent); cursor: pointer;
  }
  .gen-selall:hover:not(:disabled) { text-decoration: underline; }
  .gen-selall:disabled { color: var(--text-disabled); cursor: not-allowed; }

  .gen-target { font-size: 11.5px; color: var(--text-muted); }
  .gen-target code {
    font-family: var(--font-code);
    color: var(--text-secondary);
    font-size: 11.5px;
  }

  .gen-mock-note {
    margin: 0;
    font-size: 10.5px;
    color: var(--text-muted);
    line-height: 1.4;
  }
  .gen-mock-note strong { color: var(--text-secondary); font-weight: 600; }

  .gen-inline-note {
    margin: -2px 0 0;
    font-size: 10.5px;
    color: var(--text-muted);
    line-height: 1.4;
  }
  .gen-inline-note code { font-family: var(--font-code); font-size: 10px; }

  .gen-field-list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 3px;
    overflow-y: auto;
    min-height: 0;
    max-height: 200px;
  }
  .gen-field-list.muted { opacity: 0.55; }

  .gen-field {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 7px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast);
  }
  .gen-field:hover { background: var(--bg-hover); }
  /* Native checkbox hidden but focusable (keyboard reaches it via Tab). */
  .gen-field input {
    position: absolute; width: 1px; height: 1px; margin: -1px;
    padding: 0; border: 0; overflow: hidden; clip: rect(0 0 0 0);
  }
  .gen-field-check {
    display: flex; align-items: center; justify-content: center;
    width: 15px; height: 15px; flex-shrink: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-input);
    color: var(--text-on-accent);
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .gen-field.on .gen-field-check {
    background: var(--accent);
    border-color: var(--accent);
  }
  .gen-field input:focus-visible ~ .gen-field-check {
    box-shadow: 0 0 0 3px var(--accent-subtle);
  }
  .gen-field-name {
    font-family: var(--font-code);
    font-size: 12px;
    color: var(--text-primary);
    min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .gen-field-req {
    flex-shrink: 0;
    font-size: 8.5px;
    font-weight: 700;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    padding: 1px 4px;
    border-radius: var(--radius-sm);
    color: var(--warning);
    background: color-mix(in srgb, var(--warning) 16%, transparent);
  }

  /* G / S / W accessor-presence chips. */
  .gen-acc {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-left: auto;
    flex-shrink: 0;
  }
  .gen-acc-chip {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    border-radius: var(--radius-sm);
    font-size: 9px;
    font-weight: 700;
    font-family: var(--font-code);
    color: var(--text-disabled);
    background: var(--bg-overlay);
    border: 1px solid transparent;
  }
  .gen-acc-chip.on {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    border-color: color-mix(in srgb, var(--accent) 30%, transparent);
  }

  .gen-field-type {
    margin-left: 6px;
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .gen-opts { gap: 9px; }
  .gen-opt-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .gen-opt-name { font-size: 12px; color: var(--text-secondary); flex-shrink: 0; }

  /* ── Right column ── */
  .gen-right {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 14px;
    min-height: 0;
    min-width: 0;
  }
  .gen-preview-label { flex-shrink: 0; }
  .gen-preview {
    flex: 1;
    min-height: 0;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--bg-base);
  }
  .gen-preview :global(.cm-editor) { height: 100%; }
  .gen-preview-empty {
    height: 100%;
    display: flex; align-items: center; justify-content: center;
  }

  /* ── Footer ── */
  .gen-hint { font-size: 11px; color: var(--text-muted); }
  .gen-actions { display: flex; align-items: center; gap: 8px; }
</style>
