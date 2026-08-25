<script lang="ts">
  /**
   * BennuFormFieldRow — one input field (parameter) of a JSP `<form>` in the Forms tool window.
   *
   * A COLUMN-ALIGNED row (the panel lives in the wide bottom dock): fixed grid columns so
   * name · value · condition · source · control · badges line up vertically across every field.
   * Every cell is always rendered (empty when it has no content) so the columns never shift.
   *
   *   • name    — the parameter name (muted when it binds to nothing on the action class);
   *   • value   — the `= value` the form posts (a fixed value or `${…}`/`%{…}` expression);
   *   • IF      — a marker when the field is submitted only under a condition (tooltip = the test);
   *   • source  — the include fragment the parameter was pulled in from (when not the form's page);
   *   • control — the control kind (hidden / text / select …);
   *   • badges  — de-emphasized: the expected good state (binds AND validated) is a discreet ✓;
   *               only exceptions light up — an **unbound** warning (the field maps to nothing
   *               on the action class) and/or a **valid** chip (carries a validation rule).
   *
   * Clicking (or Enter/Space) jumps the editor to the field-name span in its source file. A dumb
   * presentational row: owns no state, imports only shared/ui.
   */
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { FileInput, Check } from 'lucide-svelte';
  import type { FormFieldInfo } from '$lib/ipc/bennu/forms';

  let {
    field,
    hostFile,
    focusFile,
    onjump,
  }: {
    field: FormFieldInfo;
    /** The form's `host_file` — when the field's `source_file` differs, it was pulled in from
     *  an include and we show which fragment it comes from. */
    hostFile?: string | null;
    /** The file the panel is analysing — a field it contributes is highlighted ("here"). */
    focusFile?: string | null;
    /** Jump the editor to this field's name span (opens its `source_file` first if cross-file). */
    onjump: () => void;
  } = $props();

  // Neither bound nor validated → the field maps to nothing on the action class: a
  // likely typo or an unmapped request parameter. Surface it as muted.
  const unmapped = $derived(!field.bound && !field.validated);
  // The expected "good" state (binds AND is validated) — shown as a discreet ✓ so it doesn't
  // shout; only the EXCEPTIONS (missing bind / missing validation) light up explicit badges.
  const allGood = $derived(field.bound && field.validated);

  /** The field was contributed by a DIFFERENT file than the form's page — show its fragment. */
  const fromInclude = $derived(
    hostFile && field.source_file && field.source_file !== hostFile
      ? (field.source_file.split('/').pop() ?? field.source_file)
      : null,
  );
  /** On a REMOTE form (you're viewing an included fragment, not the form's page), highlight the
   *  parameters this fragment contributes so "yours" stand out among the parent's whole set. */
  const fromFocus = $derived(
    !!focusFile && field.source_file === focusFile && hostFile !== focusFile,
  );
</script>

<button
  type="button"
  class="ff-row"
  class:unmapped
  class:from-focus={fromFocus}
  onclick={onjump}
  use:tooltip={unmapped ? 'Not bound or validated — check for a typo or an unmapped parameter' : undefined}
>
  <span class="ff-name">{field.name || '(unnamed)'}</span>

  <span class="ff-value" use:tooltip={field.value ?? undefined}>
    {#if field.value}<span class="ff-eq">=</span> {field.value}{/if}
  </span>

  <span class="ff-cond-cell">
    {#if field.conditional}
      <span
        class="ff-cond"
        use:tooltip={field.condition ? `Only submitted when: ${field.condition}` : 'Conditionally submitted'}
      >IF</span>
    {/if}
  </span>

  <span class="ff-src-cell">
    {#if fromInclude}
      <span class="ff-src" use:tooltip={`Included from ${field.source_file}`}>
        <FileInput size={11} />
        <span class="ff-src-name">{fromInclude}</span>
      </span>
    {/if}
  </span>

  <span class="ff-control">{field.control}</span>

  <span class="ff-badges">
    {#if allGood}
      <span class="ff-ok" use:tooltip={'Binds to the action class and is validated'}>
        <Check size={13} />
      </span>
    {:else}
      {#if !field.bound}
        <Badge variant="tone" size="sm" tone="warning" label="unbound" />
      {/if}
      {#if field.validated}
        <Badge variant="tone" size="sm" tone="info" label="valid" />
      {/if}
    {/if}
  </span>
</button>

<style>
  .ff-row {
    display: grid;
    /* name · value · IF · source · control · badges — fixed columns so every row aligns. */
    grid-template-columns:
      minmax(130px, 220px) minmax(70px, 1fr) 24px minmax(0, 210px) 58px max-content;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 2px 12px 2px 28px;
    background: transparent;
    border: none;
    border-left: 2px solid transparent;
    text-align: left;
    cursor: pointer;
    color: var(--text-secondary);
    transition: background var(--transition-fast);
  }
  .ff-row:hover { background: var(--bg-hover); }
  .ff-row:focus-visible { background: var(--bg-selected, var(--accent-subtle)); border-left-color: var(--accent); outline: none; }
  .ff-row.unmapped .ff-name { color: var(--text-muted); }
  /* A parameter the file you're viewing contributes — mark its left edge. */
  .ff-row.from-focus { border-left-color: var(--accent); }

  .ff-name {
    min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-code);
    font-size: var(--font-size-sm);
    color: var(--text-primary);
  }

  /* The submitted value — muted monospace, fills its column and truncates. */
  .ff-value {
    min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
  }
  .ff-eq { color: var(--text-muted); }

  /* Conditional marker — compact, warning-tinted text (no heavy pill: it repeats down a column). */
  .ff-cond-cell { justify-self: start; }
  .ff-cond {
    font-family: var(--font-code);
    font-size: var(--font-size-3xs); font-weight: 700; letter-spacing: 0.4px;
    color: var(--warning, #d19a66);
    cursor: help;
  }

  /* Source fragment — muted text + a small file glyph, no background (it fills a whole column). */
  .ff-src-cell { min-width: 0; }
  .ff-src {
    display: inline-flex; align-items: center; gap: 4px;
    max-width: 100%;
    color: var(--text-muted);
    font-size: var(--font-size-2xs);
    cursor: help;
  }
  .ff-src :global(svg) { flex-shrink: 0; opacity: 0.8; }
  .ff-src-name {
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-code);
  }

  .ff-control {
    justify-self: start;
    font-size: var(--font-size-3xs);
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }
  .ff-badges { display: flex; align-items: center; gap: 4px; justify-self: end; }
  /* The discreet "all good" mark — binds & validated. Muted success, no loud pills. */
  .ff-ok {
    display: inline-flex; align-items: center; justify-content: center;
    color: var(--success, #98c379);
    opacity: 0.6;
    cursor: help;
  }
</style>
