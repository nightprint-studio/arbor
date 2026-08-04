<script module lang="ts">
  import type { LanguageDescriptor as CodeTabLanguage } from '$lib/components/shared/ui/code-editor';

  /** One view of the same generated thing — the Java it writes, the DDL it implies, the query it
   *  derives. Tabs rather than three stacked blocks because they are alternatives, not parts: you
   *  read one of them, and stacking makes the pane scroll for no reason. */
  export interface CodeTab {
    id: string;
    label: string;
    code: string;
    language: CodeTabLanguage;
    /** Right-aligned secondary text while this tab is showing. */
    detail?: string;
    /** Shown in place of the code when this tab has none. */
    empty?: string;
  }
</script>

<script lang="ts">
  /**
   * CodePreview — a block of code shown, not edited.
   *
   * The read-only half of `CodeEditor`, packaged so nobody has to reach for a `<pre>` again.
   * A `<pre>` was what every generator dialog in the app used, and it was wrong for a reason
   * worth writing down: **generated code that is not highlighted does not read as code.** You
   * cannot skim it, you cannot tell an annotation from a type from a string, and the one thing a
   * preview exists for — deciding whether what is about to be written is what you meant — is
   * exactly the thing flat grey text cannot support.
   *
   * So this is the real editor, in read-only mode, with the same theme and the same highlighting
   * as the buffer behind the dialog. It costs one mount; the language descriptor is a module
   * singleton at every call site, so changing `code` re-renders without remounting.
   *
   * ## Sizing
   *
   * Grows with the content up to `maxHeight`, then scrolls — because a preview that is always
   * 300px tall is mostly empty for a one-line method, and a preview that is always the size of
   * its content pushes the form it belongs to off the screen for a generated file.
   *
   * Lines are **not wrapped**. A package name broken across two lines makes correct output look
   * like a mistake, which is worse than making the reader scroll.
   */
  import type { Snippet } from 'svelte';
  import { CodeEditor, type LanguageDescriptor } from '$lib/components/shared/ui/code-editor';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';

  interface Props {
    /** The code to show. Ignored when {@link tabs} is given — pass exactly one of the two. */
    code?: string;
    /** How to highlight it. Pass a module-singleton descriptor — a fresh object per render
     *  would remount the editor on every keystroke of whatever is driving the preview. */
    language?: LanguageDescriptor;
    /**
     * Several views of the same thing, as a tab strip in the header.
     *
     * The header shows the tabs *instead of* the title, because they are the title: "Java / DDL"
     * says more about what you are looking at than a label saying "Preview" ever did.
     */
    tabs?: CodeTab[];
    /** Header label. Omit for a bare block with no header at all. */
    title?: string;
    /** Right-aligned secondary text — where this is going, which file it came from. */
    detail?: string;
    /**
     * Fill the height the parent gives it instead of growing with the content.
     *
     * For a preview that lives in its own column beside a form: there the block's height is a
     * layout decision the parent already made, and a pane that changed height as you typed
     * would make the whole dialog twitch.
     */
    fill?: boolean;
    /** Ceiling in pixels before the block scrolls instead of growing. Ignored when `fill`. */
    maxHeight?: number;
    /** Floor in pixels, so an empty preview is still a visible, labelled space rather than a
     *  line that appears and disappears as you type. Ignored when `fill`. */
    minHeight?: number;
    /** Offer a copy button in the header. */
    copyable?: boolean;
    /** Shown in place of the code when there is none yet. */
    empty?: string;
    /** Shown in place of the code, as an error. Takes precedence over everything. */
    error?: string | null;
    /** Extra header content, before the copy button. */
    actions?: Snippet;
  }

  let {
    code = '',
    language,
    tabs,
    title,
    detail,
    fill = false,
    maxHeight = 260,
    minHeight = 72,
    copyable = true,
    empty = 'Nothing to show yet.',
    error = null,
    actions,
  }: Props = $props();

  let picked = $state('');
  // The showing tab: what was picked, or the first one. Derived rather than assigned on change, so
  // a tab list that changes under the user (a form that grows a DDL view) never leaves this
  // pointing at an id that no longer exists.
  const tab = $derived(tabs?.find((t) => t.id === picked) ?? tabs?.[0]);
  const shownCode = $derived(tab ? tab.code : code);
  const shownLanguage = $derived(tab ? tab.language : language);
  const shownDetail = $derived(tab?.detail ?? detail);
  const shownEmpty = $derived(tab?.empty ?? empty);

  /** One line of the editor at the theme's code size, plus the block's own padding. Measured
   *  rather than guessed would be nicer, but it would also make the height lag one frame behind
   *  the content — and a preview that resizes late is worse than one that is a pixel out. */
  const LINE = 19;
  const height = $derived(
    Math.min(maxHeight, Math.max(minHeight, shownCode.split('\n').length * LINE + 14)),
  );
  const hasCode = $derived(!error && shownCode.trim().length > 0 && !!shownLanguage);
</script>

<div class="cp">
  {#if title || shownDetail || actions || copyable || tabs?.length}
    <div class="cp-head">
      {#if tabs?.length}
        <div class="cp-tabs" role="tablist">
          {#each tabs as t (t.id)}
            <button
              type="button"
              role="tab"
              class="cp-tab"
              class:on={t.id === tab?.id}
              aria-selected={t.id === tab?.id}
              onclick={() => (picked = t.id)}
            >
              {t.label}
            </button>
          {/each}
        </div>
      {:else if title}
        <span class="cp-title">{title}</span>
      {/if}
      {#if shownDetail}<span class="cp-detail">{shownDetail}</span>{/if}
      <div class="cp-actions">
        {@render actions?.()}
        {#if copyable && hasCode}
          <CopyButton value={shownCode} title="Copy the generated code" />
        {/if}
      </div>
    </div>
  {/if}

  {#if error}
    <p class="cp-error" class:cp-grow={fill}>{error}</p>
  {:else if hasCode && shownLanguage}
    <div class="cp-body" class:cp-grow={fill} style={fill ? undefined : `height: ${height}px`}>
      <CodeEditor value={shownCode} language={shownLanguage} readOnly />
    </div>
  {:else}
    <p class="cp-empty" class:cp-grow={fill}>{shownEmpty}</p>
  {/if}
</div>

<style>
  .cp {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    background: var(--bg-base);
    overflow: hidden;
  }

  .cp-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 6px 5px 10px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
  }
  .cp-title {
    font-size: var(--font-size-xs);
    font-weight: 600;
    color: var(--text-secondary);
  }
  .cp-detail {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-muted);
  }
  .cp-actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 2px;
  }

  /* Tabs sit where the title would: they say what you are looking at better than a label could. */
  .cp-tabs { display: flex; align-items: center; gap: 2px; }
  .cp-tab {
    padding: 3px 9px;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    cursor: pointer;
    font: inherit;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
  .cp-tab:hover { color: var(--text-primary); background: var(--bg-hover); }
  .cp-tab.on { color: var(--text-primary); background: var(--bg-base); box-shadow: inset 0 0 0 1px var(--border-subtle); }
  .cp-tab:focus-visible { outline: 1px solid var(--accent-primary); outline-offset: -1px; }

  .cp-body {
    min-height: 0;
    /* The editor fills the block; the block owns the height. */
    display: flex;
  }
  /* `fill`: take the column, and let the editor scroll inside it. */
  .cp-grow { flex: 1; min-height: 0; overflow: auto; }
  .cp-body > :global(*) {
    flex: 1;
    min-width: 0;
  }
  /* Read-only: no caret, no active-line tint, no gutter — this is a picture of code, and the
     furniture that helps you edit only gets in the way of reading. */
  .cp-body :global(.cm-editor) { background: transparent; }
  .cp-body :global(.cm-cursor) { display: none; }
  .cp-body :global(.cm-activeLine) { background: transparent; }
  .cp-body :global(.cm-gutters) { display: none; }
  .cp-body :global(.cm-content) { padding: 7px 0; }

  .cp-empty, .cp-error {
    margin: 0;
    padding: 12px 12px 14px;
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-muted);
  }
  .cp-error {
    font-family: var(--font-code);
    color: var(--danger);
    white-space: pre-wrap;
  }
</style>
