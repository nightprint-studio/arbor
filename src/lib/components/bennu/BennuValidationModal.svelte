<script lang="ts">
  /**
   * BennuValidationModal — the "New validator" dialog for a Struts `<Action>-validation.xml`,
   * à la JPA Buddy's query builder. Pick a field (the modal offers the action's writable bean
   * properties, resolved by the backend), a validator type (the bundled Struts catalog), fill
   * its params + message, and the generated `<field><field-validator>` XML is inserted at the
   * caret. The generation is pure (`validation-xml.ts`); the field candidates come from
   * `bennu_validation_context`.
   *
   * Keyboard-first: the field input is auto-focused; Esc cancels (Modal owns it); Ctrl/Cmd+Enter
   * inserts. Reuses shared widgets only (Modal/Header/Footer, Select, Input, Toggle, Button, the
   * read-only CodeEditor for the live preview).
   */
  import { ShieldCheck } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { CodeEditor } from '$lib/components/shared/ui/code-editor';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { validationContext, type ValidationContext } from '$lib/ipc/bennu/validation';
  import { STRUTS_VALIDATORS, validatorByType } from './validation-catalog';
  import { renderFieldBlock, type ValidatorSpec } from './validation-xml';
  import { languageForPath } from './languages';

  let {
    onClose,
    onInsert,
  }: {
    onClose: () => void;
    /** Called with the generated `<field>` XML when the user confirms — the caller wires
     *  this to the editor's insert-at-caret imperative API. */
    onInsert: (text: string) => void;
  } = $props();

  // ── Backend context (action class + property candidates + existing fields) ──────
  // The modal is blocking, so the target file is whatever is active when it opens; we load
  // its context once (reading the store inside the effect avoids capturing $state at top level).
  let ctx = $state<ValidationContext | null>(null);
  let loading = $state(true);
  $effect(() => {
    const file = projectStore.activeFilePath;
    if (!file) { loading = false; return; }
    let alive = true;
    validationContext(file)
      .then((c) => { if (alive) ctx = c; })
      .catch(() => { /* unresolved → free-text field, no candidates */ })
      .finally(() => { if (alive) loading = false; });
    return () => { alive = false; };
  });

  const properties = $derived(ctx?.properties ?? []);
  const existingFields = $derived(ctx?.existing_fields ?? []);
  const actionLabel = $derived(ctx?.action_simple || 'action');

  // ── Form state ──────────────────────────────────────────────────────────────
  let field = $state('');
  let validatorType = $state(STRUTS_VALIDATORS[0].type);
  // Param values keyed by param name (text/number as string; bool as 'true'/'false').
  let paramValues = $state<Record<string, string>>({});
  let message = $state('');
  let shortCircuit = $state(false);

  const validator = $derived(validatorByType(validatorType) ?? STRUTS_VALIDATORS[0]);
  const validatorOptions = STRUTS_VALIDATORS.map((v) => ({ value: v.type, label: v.label }));

  // Reset param values when the validator changes (its param set differs).
  let lastType = '';
  $effect(() => {
    if (validatorType !== lastType) {
      lastType = validatorType;
      paramValues = {};
    }
  });

  const fieldExists = $derived(!!field && existingFields.includes(field));

  // ── Generated XML (live preview) ──────────────────────────────────────────────
  const xmlLang = languageForPath('preview.xml');

  /** The spec for the current choices — only non-empty params, bools only when on. */
  const spec = $derived.by<ValidatorSpec>(() => {
    const params: Record<string, string> = {};
    for (const p of validator.params) {
      const v = paramValues[p.name];
      if (p.kind === 'bool') {
        if (v === 'true') params[p.name] = 'true';
      } else if (v !== undefined && v.trim() !== '') {
        params[p.name] = v.trim();
      }
    }
    return {
      field: field.trim(),
      type: validatorType,
      params,
      message: message.trim() || 'Invalid value.',
      shortCircuit,
    };
  });

  const preview = $derived(field.trim() ? renderFieldBlock(spec) : '');
  const canInsert = $derived(preview.length > 0);

  function insert() {
    if (!canInsert) return;
    // A trailing newline so the block sits on its own line where the caret is.
    onInsert(preview + '\n');
    onClose();
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      insert();
    }
  }

  // Auto-focus the field input on open.
  let fieldInputEl = $state<HTMLInputElement | undefined>();
  $effect(() => { fieldInputEl?.focus(); });
</script>

<Modal {onClose} width="720px" height="560px" padBody={false} bodyBorder ariaLabel="New Struts validator">
  {#snippet header()}
    <ModalHeader {onClose}>
      <ShieldCheck size={14} />
      <span class="modal-title">New validator</span>
      <span class="vm-target">for <code>{actionLabel}</code></span>
    </ModalHeader>
  {/snippet}

  <div class="vm" onkeydown={onKey} role="presentation">
    <!-- Left column: choices -->
    <div class="vm-left">
      <section class="vm-sec">
        <span class="vm-label">Field</span>
        <input
          class="vm-field-input"
          bind:this={fieldInputEl}
          bind:value={field}
          placeholder="property name (e.g. username)"
          spellcheck="false"
          autocomplete="off"
        />
        {#if fieldExists}
          <p class="vm-note">Already has a validator in this file — a second block is appended.</p>
        {/if}

        {#if loading}
          <p class="vm-note"><Spinner size={11} /> Resolving the action's properties…</p>
        {:else if properties.length}
          <div class="vm-chips" role="group" aria-label="Action properties">
            {#each properties as p (p)}
              <button
                type="button"
                class="vm-chip"
                class:on={field === p}
                onclick={() => (field = p)}
                use:tooltip={existingFields.includes(p) ? 'Already validated' : 'Action property'}
              >
                {p}{#if existingFields.includes(p)}<span class="vm-chip-dot" aria-hidden="true"></span>{/if}
              </button>
            {/each}
          </div>
        {/if}
      </section>

      <section class="vm-sec">
        <span class="vm-label">Validator</span>
        <Select bind:value={validatorType} options={validatorOptions} onchange={(v) => (validatorType = v)} />
        <p class="vm-desc">{validator.description}</p>
      </section>

      {#if validator.params.length}
        <section class="vm-sec">
          <span class="vm-label">Parameters</span>
          {#each validator.params as p (p.name)}
            <div class="vm-param-row">
              {#if p.kind === 'bool'}
                <Toggle
                  checked={paramValues[p.name] === 'true'}
                  label={p.label}
                  onchange={(v) => (paramValues = { ...paramValues, [p.name]: v ? 'true' : 'false' })}
                />
              {:else}
                <span class="vm-param-name">{p.label}</span>
                <Input
                  type={p.kind === 'number' ? 'number' : 'text'}
                  value={paramValues[p.name] ?? ''}
                  placeholder={p.placeholder ?? ''}
                  oninput={(v) => (paramValues = { ...paramValues, [p.name]: v })}
                />
              {/if}
            </div>
            {#if p.hint && p.kind !== 'bool'}<p class="vm-hint">{p.hint}</p>{/if}
          {/each}
        </section>
      {/if}

      <section class="vm-sec">
        <span class="vm-label">Message</span>
        <Input value={message} placeholder="Error message shown on failure" oninput={(v) => (message = v)} />
        <div class="vm-sc">
          <Toggle
            checked={shortCircuit}
            label="Short-circuit"
            size="sm"
            onchange={(v) => (shortCircuit = v)}
          />
          <span class="vm-sc-hint" use:tooltip={'Stop running further validators on this field once this one fails.'}>?</span>
        </div>
      </section>
    </div>

    <!-- Right column: live preview -->
    <div class="vm-right">
      <span class="vm-label vm-preview-label">Preview</span>
      <div class="vm-preview">
        {#if canInsert}
          <CodeEditor value={preview} language={xmlLang} readOnly />
        {:else}
          <div class="vm-preview-empty">Enter a field name to preview the validator.</div>
        {/if}
      </div>
    </div>
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="vm-foot-hint">Inserts at the caret — place it inside <code>&lt;validators&gt;</code>.</span>
      <div class="vm-actions">
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
  .vm {
    display: grid;
    grid-template-columns: minmax(0, 340px) minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  .vm-left {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 14px;
    overflow-y: auto;
    border-right: 1px solid var(--border-subtle);
    min-height: 0;
  }

  .vm-sec { display: flex; flex-direction: column; gap: 7px; }

  .vm-label {
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .vm-target { font-size: 11.5px; color: var(--text-muted); }
  .vm-target code, .vm-foot-hint code {
    font-family: var(--font-code);
    color: var(--text-secondary);
    font-size: 11.5px;
  }

  /* Field input — matches the shared Input surface (Input has no free-text-with-chips mode). */
  .vm-field-input {
    width: 100%;
    padding: 6px 9px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 12.5px;
    outline: none;
  }
  .vm-field-input:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-subtle); }
  .vm-field-input::placeholder { color: var(--text-disabled); }

  .vm-note {
    margin: 0;
    display: flex; align-items: center; gap: 5px;
    font-size: 10.5px; color: var(--text-muted); line-height: 1.4;
  }

  .vm-chips { display: flex; flex-wrap: wrap; gap: 5px; }
  .vm-chip {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 3px 8px;
    background: var(--bg-overlay);
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    font-family: var(--font-code);
    font-size: 11px;
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .vm-chip:hover { background: var(--bg-hover); }
  .vm-chip.on {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border-color: color-mix(in srgb, var(--accent) 30%, transparent);
  }
  .vm-chip-dot { width: 5px; height: 5px; border-radius: 50%; background: var(--warning); }

  .vm-desc, .vm-hint {
    margin: 0;
    font-size: 10.5px;
    color: var(--text-muted);
    line-height: 1.4;
  }

  .vm-param-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .vm-param-name { font-size: 12px; color: var(--text-secondary); flex-shrink: 0; }
  .vm-param-row :global(.input-wrap) { max-width: 160px; }

  .vm-sc { display: flex; align-items: center; gap: 6px; margin-top: 2px; }
  .vm-sc-hint {
    display: inline-flex; align-items: center; justify-content: center;
    width: 15px; height: 15px; border-radius: 50%;
    font-size: 10px; color: var(--text-muted);
    background: var(--bg-overlay); cursor: help;
  }

  .vm-right {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 14px;
    min-height: 0; min-width: 0;
  }
  .vm-preview-label { flex-shrink: 0; }
  .vm-preview {
    flex: 1;
    min-height: 0;
    display: flex;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--bg-base);
  }
  .vm-preview > :global(.code-editor) { flex: 1; min-width: 0; min-height: 0; }
  .vm-preview :global(.cm-editor) { height: 100%; }
  .vm-preview-empty {
    height: 100%;
    display: flex; align-items: center; justify-content: center;
    padding: 20px; text-align: center;
    font-size: 11.5px; color: var(--text-muted);
  }

  .vm-foot-hint { font-size: 11px; color: var(--text-muted); }
  .vm-actions { display: flex; align-items: center; gap: 8px; }
</style>
