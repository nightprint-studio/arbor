<script lang="ts">
  /**
   * MarkdownToolbar — the thin strip of formatting buttons over a markdown editor.
   *
   * **Not the primary interface.** Every button is a command first and most have a chord
   * (`Alt+B`, …) shown in its tooltip; the strip exists so the chords are discoverable and a
   * pointer can reach them, not to be the way text gets formatted.
   *
   * **Toggles, driven by the caret.** `state` says what the selection is in (bold, a list item,
   * a heading, a code block), and the matching buttons are drawn pressed; pressing one again
   * removes the format — the command is the same function the keymap runs
   * (`MARKDOWN_ACTIONS` in `$lib/utils/markdown-format`), so a button and its chord cannot
   * drift apart. The host feeds `state` from `MarkdownEditor`'s `onFormatState`.
   *
   * It owns no editor: a click hands a command to `onCommand`, and the host runs it on the
   * view it holds (`MarkdownEditor.exec`). Used by Garrulus's notes and Bennu's `.md` tabs.
   */
  import type { Component, Snippet } from 'svelte';
  import {
    Bold, Braces, Code, Highlighter, Italic, Link, List, ListChecks, ListOrdered, Quote,
    Strikethrough,
  } from 'lucide-svelte';
  import IconButton from './IconButton.svelte';
  import Select from './Select.svelte';
  import TableSizePicker from './TableSizePicker.svelte';
  import {
    markdownAction, setHeading, insertTableOfSize, NO_FORMAT, type EditorCommand, type FormatState,
  } from '$lib/utils/markdown-format';

  interface Props {
    onCommand: (command: EditorCommand) => void;
    /** What the selection is in. Omitted → nothing is drawn pressed. */
    state?: FormatState;
    /** Greys the strip — a note shown in reading mode has no caret to format. */
    disabled?: boolean;
    /** Action ids to leave out. */
    hide?: string[];
    /** Host buttons appended after the built-in groups (a vault's "link to a note"). */
    extra?: Snippet;
  }

  let { onCommand, state = NO_FORMAT, disabled = false, hide = [], extra }: Props = $props();

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const ICONS: Record<string, Component<any>> = {
    bold: Bold, italic: Italic, strike: Strikethrough, highlight: Highlighter, code: Code,
    quote: Quote, bullet: List, numbered: ListOrdered, task: ListChecks,
    link: Link, codeblock: Braces,
  };

  /** Button groups, by action id; the heading picker sits between the first and second. */
  const GROUPS: string[][] = [
    ['bold', 'italic', 'strike', 'highlight', 'code'],
    ['quote', 'bullet', 'numbered', 'task'],
    ['link', 'codeblock', 'table'],
  ];

  /** The hue each button wears, so the strip reads by colour before it reads by shape: emphasis
   *  blue, the highlighter yellow, lists green, structure (quote, block, table) in the code
   *  palette. A row of identical grey outlines made the reader hover each one to find it. */
  const TINTS: Record<string, string> = {
    bold: 'var(--info)', italic: 'var(--info)', strike: 'var(--info)',
    highlight: 'var(--warning)', code: 'var(--syntax-string, #6a8759)',
    quote: 'var(--syntax-type, #4ec9b0)',
    bullet: 'var(--success)', numbered: 'var(--success)', task: 'var(--success)',
    link: 'var(--accent)', codeblock: 'var(--syntax-keyword, #cc7832)',
    table: 'var(--syntax-function, #ffc66d)',
  };

  const groups = $derived(GROUPS.map((g) => g.filter((id) => !hide.includes(id))).filter((g) => g.length));

  const HEADING_OPTIONS = [
    { value: '0', label: 'Text' },
    ...[1, 2, 3, 4, 5, 6].map((n) => ({ value: String(n), label: `Heading ${n}` })),
  ];

  const canApply = $derived(!disabled);
</script>

<div class="mdt" role="toolbar" aria-label="Formatting" aria-disabled={disabled}>
  <div class="mdt-heading" title="Heading level of the current line">
    <Select
      value={String(state.heading)}
      options={HEADING_OPTIONS}
      size="sm"
      quiet
      highlight={state.heading > 0}
      disabled={!canApply}
      onchange={(v) => onCommand(setHeading(Number(v)))}
    />
  </div>
  {#each groups as group, i (i)}
    <span class="mdt-sep" aria-hidden="true"></span>
    {#each group as id (id)}
      {@const action = markdownAction(id)}
      {@const Icon = ICONS[id]}
      {#if id === 'table'}
        <!-- A size, not a default: the grid you point a width and a height out of. -->
        <TableSizePicker
          tooltip="Table"
          shortcut={action.shortcut}
          size={22}
          color={TINTS.table}
          disabled={!canApply}
          onPick={(rows, cols) => onCommand(insertTableOfSize(cols, rows))}
        />
      {:else}
        <!-- mousedown is held back so the click does not pull focus (and the selection) out of
             the editor before the command reads it. -->
        <span class="mdt-slot" role="presentation" onmousedown={(e) => e.preventDefault()}>
          <IconButton
            swell
            tooltip={action.label}
            shortcut={action.shortcut}
            size={22}
            disabled={!canApply}
            active={action.active?.(state) ?? false}
            onclick={() => onCommand(action.command)}
          >
            <span class="mdt-ico" style:color={TINTS[id]}><Icon size={13} /></span>
          </IconButton>
        </span>
      {/if}
    {/each}
  {/each}
  {#if extra}
    <span class="mdt-sep" aria-hidden="true"></span>
    {@render extra()}
  {/if}
</div>

<style>
  .mdt {
    display: flex;
    align-items: center;
    gap: 1px;
    min-width: 0;
  }
  .mdt-slot { display: inline-flex; }
  .mdt-ico { display: inline-flex; align-items: center; justify-content: center; }
  .mdt-heading { display: inline-flex; flex: none; margin-right: 2px; }
  .mdt-sep {
    width: 1px;
    height: 14px;
    margin: 0 5px;
    background: var(--border-subtle);
  }
</style>
