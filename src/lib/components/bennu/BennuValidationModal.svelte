<script lang="ts">
  /**
   * BennuValidationModal — the Struts `<Action>-validation.xml` **chain builder**.
   *
   * Pick a field (the action's writable bean properties, resolved by the backend) and build an
   * ordered **chain** of validators for it — each with its own params, message and short-circuit —
   * then apply it into the open validation document. Both the live preview and the write go through
   * the backend authoring (`bennu_validation_author`), so what you see is exactly what is written,
   * and the XML generation is the same unit-tested Rust for every path. The validator vocabulary +
   * per-type params come from `bennu_validator_catalog` (one source of truth).
   *
   * Keyboard-first: the field input auto-focuses; Esc cancels (Modal owns it); Ctrl/Cmd+Enter
   * applies. Reuses shared widgets only (Modal/Header/Footer, Select, Input, Toggle, Button, the
   * read-only CodeEditor for the preview).
   */
  import { ShieldCheck, Plus, Trash2, ArrowUp, ArrowDown } from 'lucide-svelte';
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
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import {
    validationContext, validatorCatalog, validationAuthor,
    type ValidationContext, type ValidatorDef, type AuthoredValidator,
  } from '$lib/ipc/bennu/validation';
  import { languageForPath } from './languages';

  let { onClose }: { onClose: () => void } = $props();

  // The target validation file is whatever is active when the modal opens.
  const targetFile = projectStore.activeFilePath;

  // ── Backend data: field candidates + the validator catalog ──────────────────────
  let ctx = $state<ValidationContext | null>(null);
  let loading = $state(true);
  let catalog = $state<ValidatorDef[]>([]);
  $effect(() => {
    let alive = true;
    if (targetFile) {
      validationContext(targetFile).then((c) => { if (alive) ctx = c; }).catch(() => {}).finally(() => { if (alive) loading = false; });
    } else {
      loading = false;
    }
    validatorCatalog().then((c) => { if (alive) catalog = c; }).catch(() => {});
    return () => { alive = false; };
  });

  const properties = $derived(ctx?.properties ?? []);
  const existingFields = $derived(ctx?.existing_fields ?? []);
  const actionLabel = $derived(ctx?.action_simple || 'action');
  // Only field validators belong in a `<field>` chain (the non-field `expression` is excluded).
  const fieldValidators = $derived(catalog.filter((v) => v.is_field));
  const validatorOptions = $derived(fieldValidators.map((v) => ({ value: v.type_name, label: v.label })));

  function defFor(type: string): ValidatorDef | undefined {
    return catalog.find((v) => v.type_name === type);
  }

  // ── Chain state ─────────────────────────────────────────────────────────────
  interface ChainItem {
    type: string;
    /** Param values keyed by param name (bool as 'true'/'false'; others as text). */
    params: Record<string, string>;
    message: string;
    shortCircuit: boolean;
  }
  let field = $state('');
  let chain = $state<ChainItem[]>([]);

  // Seed one validator once the catalog lands (so the builder isn't empty on open).
  let seeded = false;
  $effect(() => {
    if (!seeded && fieldValidators.length) {
      seeded = true;
      chain = [{ type: fieldValidators[0].type_name, params: {}, message: '', shortCircuit: false }];
    }
  });

  function addValidator() {
    const type = fieldValidators[0]?.type_name ?? 'required';
    chain = [...chain, { type, params: {}, message: '', shortCircuit: false }];
  }
  function removeValidator(i: number) {
    chain = chain.filter((_, idx) => idx !== i);
  }
  function move(i: number, dir: -1 | 1) {
    const j = i + dir;
    if (j < 0 || j >= chain.length) return;
    const next = [...chain];
    [next[i], next[j]] = [next[j], next[i]];
    chain = next;
  }
  function patch(i: number, p: Partial<ChainItem>) {
    chain = chain.map((c, idx) => (idx === i ? { ...c, ...p } : c));
  }
  function setType(i: number, type: string) {
    // Reset params when the type changes (its param set differs).
    chain = chain.map((c, idx) => (idx === i ? { ...c, type, params: {} } : c));
  }
  function setParam(i: number, name: string, value: string) {
    chain = chain.map((c, idx) => (idx === i ? { ...c, params: { ...c.params, [name]: value } } : c));
  }

  /** Build the wire chain (drop empty params; bools only when true; default message text). */
  function toAuthored(): AuthoredValidator[] {
    return chain.map((c) => {
      const def = defFor(c.type);
      const params: { name: string; value: string }[] = [];
      for (const p of def?.params ?? []) {
        const v = c.params[p.name];
        if (p.kind === 'bool') {
          if (v === 'true') params.push({ name: p.name, value: 'true' });
        } else if (v !== undefined && v.trim() !== '') {
          params.push({ name: p.name, value: v.trim() });
        }
      }
      return {
        type_name: c.type,
        params,
        message: { key: null, text: c.message.trim() || 'Invalid value.' },
        short_circuit: c.shortCircuit,
      };
    });
  }

  const canApply = $derived(!!field.trim() && chain.length > 0);
  const fieldExists = $derived(!!field && existingFields.includes(field));

  // ── Live preview (debounced) — the `<field>` block the backend would author ─────
  const xmlLang = languageForPath('preview.xml');
  let preview = $state('');
  $effect(() => {
    const f = field.trim();
    const validators = toAuthored(); // reads chain → establishes reactive deps
    if (!f || !validators.length) { preview = ''; return; }
    let alive = true;
    const t = setTimeout(() => {
      validationAuthor('', f, validators)
        .then((xml) => { if (alive) preview = extractFieldBlock(xml); })
        .catch(() => { if (alive) preview = ''; });
    }, 150);
    return () => { alive = false; clearTimeout(t); };
  });

  /** Slice out the `<field>…</field>` block from a full authored document, for the preview. */
  function extractFieldBlock(xml: string): string {
    const start = xml.indexOf('<field ');
    const end = xml.indexOf('</field>');
    if (start < 0 || end < 0) return xml.trim();
    return xml.slice(start, end + '</field>'.length);
  }

  // ── Apply — author into the open document + save ────────────────────────────
  let applying = $state(false);
  async function apply() {
    if (!canApply || !targetFile || applying) return;
    applying = true;
    try {
      const buffer = projectStore.sourceOf(targetFile);
      const newXml = await validationAuthor(buffer, field.trim(), toAuthored());
      await projectStore.saveText(targetFile, newXml);
      toastStore.show(`Added ${chain.length} validator${chain.length === 1 ? '' : 's'} to “${field.trim()}”`, 'success');
      onClose();
    } catch {
      toastStore.show('Could not write the validation file', 'error');
    } finally {
      applying = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); void apply(); }
  }

  let fieldInputEl = $state<HTMLInputElement | undefined>();
  $effect(() => { fieldInputEl?.focus(); });
</script>

<Modal {onClose} width="760px" height="580px" padBody={false} bodyBorder ariaLabel="Struts validator chain">
  {#snippet header()}
    <ModalHeader {onClose}>
      <ShieldCheck size={14} />
      <span class="modal-title">Validators</span>
      <span class="vm-target">for <code>{actionLabel}</code></span>
    </ModalHeader>
  {/snippet}

  <div class="vm" onkeydown={onKey} role="presentation">
    <!-- Left: field + validator chain -->
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
          <p class="vm-note">Already validated in this file — the chain is appended to it.</p>
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
        <div class="vm-chain-head">
          <span class="vm-label">Validator chain</span>
          <Button variant="ghost" size="xs" onclick={addValidator}>
            {#snippet iconStart()}<Plus size={13} />{/snippet}
            Add
          </Button>
        </div>

        {#each chain as item, i (i)}
          {@const def = defFor(item.type)}
          <div class="vm-card">
            <div class="vm-card-head">
              <span class="vm-card-idx">{i + 1}</span>
              <div class="vm-card-select">
                <Select value={item.type} options={validatorOptions} onchange={(v) => setType(i, v)} />
              </div>
              <button class="vm-icon" disabled={i === 0} onclick={() => move(i, -1)} use:tooltip={'Move up'} aria-label="Move up"><ArrowUp size={13} /></button>
              <button class="vm-icon" disabled={i === chain.length - 1} onclick={() => move(i, 1)} use:tooltip={'Move down'} aria-label="Move down"><ArrowDown size={13} /></button>
              <button class="vm-icon vm-icon-danger" disabled={chain.length === 1} onclick={() => removeValidator(i)} use:tooltip={'Remove'} aria-label="Remove"><Trash2 size={13} /></button>
            </div>

            {#if def?.params.length}
              <div class="vm-params">
                {#each def.params as p (p.name)}
                  <div class="vm-param-row">
                    {#if p.kind === 'bool'}
                      <Toggle
                        checked={item.params[p.name] === 'true'}
                        label={p.name}
                        size="sm"
                        onchange={(v) => setParam(i, p.name, v ? 'true' : 'false')}
                      />
                    {:else}
                      <span class="vm-param-name">{p.name}{#if p.required}<span class="vm-req">*</span>{/if}</span>
                      <Input
                        type={p.kind === 'int' || p.kind === 'long' || p.kind === 'double' ? 'number' : 'text'}
                        value={item.params[p.name] ?? ''}
                        oninput={(v) => setParam(i, p.name, v)}
                      />
                    {/if}
                  </div>
                {/each}
              </div>
            {/if}

            <div class="vm-card-msg">
              <Input value={item.message} placeholder="Message on failure" oninput={(v) => patch(i, { message: v })} />
              <Toggle
                checked={item.shortCircuit}
                label="Short-circuit"
                size="sm"
                onchange={(v) => patch(i, { shortCircuit: v })}
              />
            </div>
          </div>
        {/each}
      </section>
    </div>

    <!-- Right: live preview -->
    <div class="vm-right">
      <span class="vm-label vm-preview-label">Preview</span>
      <div class="vm-preview">
        {#if preview}
          <CodeEditor value={preview} language={xmlLang} readOnly />
        {:else}
          <div class="vm-preview-empty">Enter a field name to preview the chain.</div>
        {/if}
      </div>
    </div>
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="vm-foot-hint">Appended into <code>{actionLabel}-validation.xml</code>.</span>
      <div class="vm-actions">
        <Button variant="ghost" size="sm" onclick={onClose}>Cancel</Button>
        <Button
          variant="primary"
          size="sm"
          disabled={!canApply}
          loading={applying}
          tooltip={{ content: 'Apply', shortcut: 'Ctrl+Enter' }}
          onclick={apply}
        >
          Add to file
        </Button>
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .vm { display: grid; grid-template-columns: minmax(0, 400px) minmax(0, 1fr); height: 100%; min-height: 0; }
  .vm-left { display: flex; flex-direction: column; gap: 14px; padding: 14px; overflow-y: auto; border-right: 1px solid var(--border-subtle); min-height: 0; }
  .vm-sec { display: flex; flex-direction: column; gap: 7px; }
  .vm-label { font-size: var(--font-size-2xs); font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase; color: var(--text-muted); }
  .vm-target { font-size: var(--font-size-xs); color: var(--text-muted); }
  .vm-target code, .vm-foot-hint code { font-family: var(--font-code); color: var(--text-secondary); font-size: var(--font-size-xs); }

  .vm-field-input { width: 100%; padding: 6px 9px; background: var(--bg-input); border: 1px solid var(--border); border-radius: var(--radius-md); color: var(--text-primary); font-family: var(--font-code); font-size: var(--font-size-sm); outline: none; }
  .vm-field-input:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-subtle); }
  .vm-field-input::placeholder { color: var(--text-disabled); }
  .vm-note { margin: 0; display: flex; align-items: center; gap: 5px; font-size: var(--font-size-2xs); color: var(--text-muted); line-height: 1.4; }

  .vm-chips { display: flex; flex-wrap: wrap; gap: 5px; }
  .vm-chip { display: inline-flex; align-items: center; gap: 4px; padding: 3px 8px; background: var(--bg-overlay); border: 1px solid transparent; border-radius: var(--radius-md); color: var(--text-secondary); font-family: var(--font-code); font-size: var(--font-size-xs); cursor: pointer; transition: background var(--transition-fast), border-color var(--transition-fast); }
  .vm-chip:hover { background: var(--bg-hover); }
  .vm-chip.on { color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, transparent); border-color: color-mix(in srgb, var(--accent) 30%, transparent); }
  .vm-chip-dot { width: 5px; height: 5px; border-radius: 50%; background: var(--warning); }

  .vm-chain-head { display: flex; align-items: center; justify-content: space-between; }

  .vm-card { display: flex; flex-direction: column; gap: 9px; padding: 10px; background: var(--bg-elevated); border: 1px solid var(--border-subtle); border-radius: var(--radius-md); }
  .vm-card-head { display: flex; align-items: center; gap: 6px; }
  .vm-card-idx { flex-shrink: 0; width: 18px; height: 18px; display: flex; align-items: center; justify-content: center; font-size: var(--font-size-2xs); font-weight: 700; color: var(--text-muted); background: var(--bg-overlay); border-radius: 5px; }
  .vm-card-select { flex: 1; min-width: 0; }
  .vm-icon { display: inline-flex; align-items: center; justify-content: center; width: 24px; height: 24px; border: none; background: transparent; color: var(--text-muted); border-radius: var(--radius-sm); cursor: pointer; transition: background var(--transition-fast), color var(--transition-fast); }
  .vm-icon:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .vm-icon:disabled { opacity: 0.35; cursor: default; }
  .vm-icon-danger:hover:not(:disabled) { background: var(--error-subtle); color: var(--error); }

  .vm-params { display: flex; flex-direction: column; gap: 6px; padding-left: 24px; }
  .vm-param-row { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .vm-param-name { font-size: var(--font-size-sm); color: var(--text-secondary); flex-shrink: 0; font-family: var(--font-code); }
  .vm-req { color: var(--error); margin-left: 2px; }
  .vm-param-row :global(.input-wrap) { max-width: 170px; }

  .vm-card-msg { display: flex; flex-direction: column; gap: 7px; padding-left: 24px; }

  .vm-right { display: flex; flex-direction: column; gap: 7px; padding: 14px; min-height: 0; min-width: 0; }
  .vm-preview-label { flex-shrink: 0; }
  .vm-preview { flex: 1; min-height: 0; display: flex; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); overflow: hidden; background: var(--bg-base); }
  .vm-preview > :global(.code-editor) { flex: 1; min-width: 0; min-height: 0; }
  .vm-preview :global(.cm-editor) { height: 100%; }
  .vm-preview-empty { height: 100%; display: flex; align-items: center; justify-content: center; padding: 20px; text-align: center; font-size: var(--font-size-xs); color: var(--text-muted); }

  .vm-foot-hint { font-size: var(--font-size-xs); color: var(--text-muted); }
  .vm-actions { display: flex; align-items: center; gap: 8px; }
</style>
