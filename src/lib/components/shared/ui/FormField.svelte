<script lang="ts">
  import type { Snippet } from 'svelte';

  /**
   * Vertical form field: label on top, control below, optional hint/error
   * underneath. Standardises the `.field-label` + `.field-group` pattern
   * duplicated across every modal. For horizontal label↔control rows use
   * `FormRow` instead.
   */
  interface Props {
    label?:        string;
    /** Small muted text after the label (e.g. "(optional)"). */
    optionalText?: string;
    /** Show a red asterisk after the label. */
    required?:     boolean;
    /** Description shown between label and control (sentence-case secondary). */
    description?:  string;
    /** Hint shown below the control. String or rich snippet (for <code>, <strong>…). */
    hint?:         string;
    hintContent?:  Snippet;
    /** Error shown below the control. Replaces the hint when present. */
    error?:        string | null;
    /** Leading icon snippet rendered before the label text. */
    icon?:         Snippet;
    /** Right-aligned content on the same row as the label (e.g. action button). */
    actions?:      Snippet;
    /** htmlFor target id on the underlying <label>. */
    for?:          string;
    children:      Snippet;
  }

  let {
    label,
    optionalText,
    required = false,
    description,
    hint,
    hintContent,
    error = null,
    icon,
    actions,
    for: htmlFor,
    children,
  }: Props = $props();

  const hasLabelRow = $derived(!!(label || icon || actions));
</script>

<div class="form-field">
  {#if hasLabelRow}
    <div class="ff-label-row">
      <label class="ff-label" for={htmlFor}>
        {#if icon}<span class="ff-icon">{@render icon()}</span>{/if}
        {#if label}<span class="ff-text">{label}</span>{/if}
        {#if required}<span class="ff-required" aria-hidden="true">*</span>{/if}
        {#if optionalText}<span class="ff-optional">{optionalText}</span>{/if}
      </label>
      {#if actions}<div class="ff-actions">{@render actions()}</div>{/if}
    </div>
  {/if}

  {#if description}
    <div class="ff-description">{description}</div>
  {/if}

  <div class="ff-control">{@render children()}</div>

  {#if error}
    <div class="ff-error">{error}</div>
  {:else if hintContent}
    <div class="ff-hint">{@render hintContent()}</div>
  {:else if hint}
    <div class="ff-hint">{hint}</div>
  {/if}
</div>

<style>
  .form-field {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }

  .ff-label-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    min-width: 0;
  }

  .ff-label {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    letter-spacing: 0.02em;
    line-height: 1.4;
    min-width: 0;
  }
  .ff-icon { display: inline-flex; align-items: center; color: var(--text-muted); flex-shrink: 0; }
  .ff-text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ff-required { color: var(--error); font-weight: 600; }
  .ff-optional {
    font-size: 10.5px;
    font-weight: 400;
    color: var(--text-muted);
    letter-spacing: 0;
  }

  .ff-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .ff-description {
    font-size: 11px;
    color: var(--text-muted);
    line-height: 1.45;
  }

  .ff-control {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .ff-hint {
    font-size: 11px;
    color: var(--text-muted);
    line-height: 1.45;
  }
  .ff-hint :global(strong) { color: var(--text-primary); font-weight: 600; }
  .ff-hint :global(code) {
    font-family: var(--font-code);
    font-size: 10.5px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    padding: 0 4px;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
  }

  .ff-error {
    font-size: 11px;
    color: var(--error);
    line-height: 1.45;
  }
</style>
