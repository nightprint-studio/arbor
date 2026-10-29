<script lang="ts">
  /**
   * The code blocks in the message being written.
   *
   * A text field cannot colour its own text, so the fenced blocks are listed above it: the language, how
   * many lines, the first line — and a click opens the block highlighted and read-only, with the
   * remove button on the row for when a paste was not meant to go in. The text itself stays in the
   * field, where it can still be edited; this is the view of it that can be read.
   */
  import { X, Code } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import AssistantCode from './AssistantCode.svelte';
  import { findFences, removeFence } from './assistant-fences';

  let { draft = $bindable('') }: { draft?: string } = $props();

  // Re-read only when the text holds a fence at all: most messages never do.
  const blocks = $derived(draft.includes('```') ? findFences(draft) : []);
  let openAt = $state<number | null>(null);
  const opened = $derived(openAt === null ? null : (blocks.find((b) => b.start === openAt) ?? null));

  const firstLine = (code: string) => code.split('\n').find((l) => l.trim() !== '')?.trim() ?? '';
  const linesLabel = (n: number) => (n === 1 ? '1 line' : `${n} lines`);
</script>

{#if blocks.length > 0}
  <ul class="blocks" aria-label="Code blocks in this message">
    {#each blocks as b (b.start)}
      <li class="row">
        <Button variant="ghost" size="xs" block tooltip="Show the block, highlighted" onclick={() => (openAt = b.start)}>
          <span class="lbl">
            <Code size={11} />
            <Badge variant="chip" size="sm" label={b.language || 'text'} />
            <span class="meta">{linesLabel(b.lines)}</span>
            <span class="first">{firstLine(b.code)}</span>
          </span>
        </Button>
        <IconButton tooltip="Remove this block" size={20} onclick={() => (draft = removeFence(draft, b))}><X size={11} /></IconButton>
      </li>
    {/each}
  </ul>
{/if}

{#if opened}
  <Modal onClose={() => (openAt = null)} width="720px" height="520px" ariaLabel="Code block">
    {#snippet header()}
      <ModalHeader onClose={() => (openAt = null)}>
        <Code size={14} />
        <span class="title">{opened.language || 'text'} · {linesLabel(opened.lines)}</span>
      </ModalHeader>
    {/snippet}
    <div class="preview"><AssistantCode language={opened.language || 'text'} code={opened.code} /></div>
  </Modal>
{/if}

<style>
  .blocks { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; max-height: 96px; overflow-y: auto; }
  .row { display: flex; align-items: center; gap: 2px; min-width: 0; }
  .lbl { display: inline-flex; align-items: center; gap: 6px; min-width: 0; max-width: 100%; }
  .meta { color: var(--text-muted); font-size: var(--font-size-2xs); flex-shrink: 0; }
  .first {
    min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-secondary);
  }
  .title { font-size: var(--font-size-sm); font-weight: 600; }
  .preview { min-height: 0; }
</style>
