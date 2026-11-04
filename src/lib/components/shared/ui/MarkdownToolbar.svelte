<script lang="ts">
  /**
   * MarkdownToolbar — the thin strip of formatting buttons over a markdown editor.
   *
   * **Not the primary interface.** Every button is a command first and has a chord
   * (`Alt+B`, …) shown in its tooltip; the strip exists so the chords are discoverable and so a
   * pointer can reach them, not to be the way text gets formatted.
   *
   * It owns no editor: a click hands a command to `onCommand`, and the host runs it on whatever
   * view it holds (`MarkdownEditor.exec`). The commands live in `$lib/utils/markdown-format`, the
   * same functions the keymap runs, so a button and its chord cannot drift apart.
   */
  import type { Snippet } from 'svelte';
  import {
    Bold, Braces, Code, Heading2, Highlighter, Italic, Link, List, ListChecks, ListOrdered,
    Quote, Strikethrough, Table2,
  } from 'lucide-svelte';
  import IconButton from './IconButton.svelte';
  import {
    insertCodeBlock, insertLink, insertTable, quoteOrCallout, setHeading, toggleBullet,
    toggleInline, toggleNumbered, toggleTask, type EditorCommand,
  } from '$lib/utils/markdown-format';

  interface Props {
    onCommand: (command: EditorCommand) => void;
    /** Greys the strip — a note shown in reading mode has no caret to format. */
    disabled?: boolean;
    /** Host buttons appended after the built-in groups (a vault's "link to a note"). */
    extra?: Snippet;
  }

  let { onCommand, disabled = false, extra }: Props = $props();

  interface Tool {
    tip: string;
    shortcut?: string;
    icon: typeof Bold;
    command: EditorCommand;
  }

  const groups: Tool[][] = [
    [
      { tip: 'Bold', shortcut: 'Alt+B', icon: Bold, command: toggleInline('**') },
      { tip: 'Italic', shortcut: 'Alt+I', icon: Italic, command: toggleInline('*') },
      { tip: 'Strikethrough', shortcut: 'Alt+S', icon: Strikethrough, command: toggleInline('~~') },
      { tip: 'Highlight', shortcut: 'Alt+H', icon: Highlighter, command: toggleInline('==') },
      { tip: 'Inline code', shortcut: 'Alt+C', icon: Code, command: toggleInline('`') },
    ],
    [
      { tip: 'Heading', icon: Heading2, command: setHeading(2) },
      { tip: 'Quote — press again for a callout', shortcut: 'Alt+Q', icon: Quote, command: quoteOrCallout },
      { tip: 'Bulleted list', icon: List, command: toggleBullet },
      { tip: 'Numbered list', icon: ListOrdered, command: toggleNumbered },
      { tip: 'Task', shortcut: 'Alt+T', icon: ListChecks, command: toggleTask },
    ],
    [
      { tip: 'Link', shortcut: 'Alt+L', icon: Link, command: insertLink },
      { tip: 'Code block', icon: Braces, command: insertCodeBlock },
      { tip: 'Table', shortcut: 'Ctrl+Shift+T', icon: Table2, command: insertTable },
    ],
  ];
</script>

<div class="mdt" role="toolbar" aria-label="Formatting" aria-disabled={disabled}>
  {#each groups as group, i (i)}
    {#if i > 0}<span class="mdt-sep" aria-hidden="true"></span>{/if}
    {#each group as tool (tool.tip)}
      {@const Icon = tool.icon}
      <!-- mousedown is held back so the click does not pull focus (and the selection) out of
           the editor before the command reads it. -->
      <span class="mdt-slot" role="presentation" onmousedown={(e) => e.preventDefault()}>
        <IconButton
          tooltip={tool.tip}
          shortcut={tool.shortcut}
          size={22}
          {disabled}
          onclick={() => onCommand(tool.command)}
        >
          <Icon size={13} />
        </IconButton>
      </span>
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
  .mdt-sep {
    width: 1px;
    height: 14px;
    margin: 0 5px;
    background: var(--border-subtle);
  }
</style>
