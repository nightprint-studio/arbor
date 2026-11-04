<script lang="ts">
  /**
   * One message of an Imber conversation, as every product draws it.
   *
   * Yours is a soft bubble on the right, the way it is in every chat — short and yours, so it can be small.
   * Imber's has no bubble: it is the content, and a frame around paragraphs and tool receipts would only
   * add edges. A quiet line above names who wrote it and when, and the blocks follow in the order the
   * product renders them (text, receipts, cards). Hovering the message reveals what you can do with it:
   * copy, and whatever extra verbs the product adds.
   *
   * The frame knows nothing about a product's block types: `children` is the assistant's blocks (or nothing
   * for a user message, whose text is `userText`).
   */
  import type { Snippet } from 'svelte';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { Copy } from 'lucide-svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { animStore } from '$lib/stores/animations.svelte';
  import ImberUserText from './ImberUserText.svelte';
  import ImberWho from './ImberWho.svelte';
  import { hasFence, inlineSpans } from './imber-fences';
  import './imber-motion.css';

  type Tone = 'info' | 'success' | 'warning' | 'error' | 'accent' | 'neutral';

  let { role, name = 'Imber', kind = 'imber', badge = null, when = '', live = false, plainText = '', userText = '', userTop, children, actions }: {
    role: 'user' | 'assistant';
    name?: string;
    kind?: 'imber' | 'agent';
    badge?: { label: string; tone: Tone } | null;
    when?: string;
    /** This author is still writing: the avatar glows. */
    live?: boolean;
    /** What "Copy" puts on the clipboard. */
    plainText?: string;
    /** A user message's text: inline code and bold in a bubble, or the full renderer when it holds a fenced block. */
    userText?: string;
    /** Above a user's bubble, right-aligned (attached files). */
    userTop?: Snippet;
    children?: Snippet;
    /** More verbs next to Copy, shown on hover. */
    actions?: Snippet;
  } = $props();

  const copy = () => copyToClipboard(plainText, { successToast: 'Copied' });
</script>

<div class="msg" in:fly={{ y: 8, duration: animStore.dFast, easing: cubicOut }}>
  {#if role === 'user'}
    {#if userTop}<div class="attached">{@render userTop()}</div>{/if}
    <div class="user">
      <div class="user-actions">
        <IconButton tooltip="Copy" size={22} onclick={copy}><Copy size={12} /></IconButton>
      </div>
      {#if hasFence(userText)}
        <div class="bubble rich"><ImberUserText text={userText} /></div>
      {:else if userText}
        <p class="bubble">{#each inlineSpans(userText) as s}{#if s.code}<code>{s.text}</code>{:else if s.bold}<strong>{s.text}</strong>{:else}{s.text}{/if}{/each}</p>
      {/if}
    </div>
  {:else}
    <div class="assistant">
      <ImberWho {name} {kind} {badge} {live} {when} />
      <div class="blocks">{@render children?.()}</div>
      <div class="hover">
        <IconButton tooltip="Copy" size={22} onclick={copy}><Copy size={12} /></IconButton>
        {@render actions?.()}
      </div>
    </div>
  {/if}
</div>

<style>
  .user { display: flex; justify-content: flex-end; align-items: flex-start; gap: 4px; }
  /* The files sit on a line of their own above the bubble, right-aligned like it. */
  .attached { display: flex; justify-content: flex-end; margin-bottom: 4px; }
  .user-actions { order: -1; opacity: 0; transition: opacity var(--transition-fast); align-self: center; }
  .user:hover .user-actions, .user:focus-within .user-actions { opacity: 1; }
  /* A message with code in it takes the width of the column: code is read wide. */
  .bubble.rich { flex: 1; min-width: 0; max-width: 100%; }
  .bubble {
    margin: 0; max-width: 90%;
    padding: 8px 12px;
    border-radius: 14px 14px 4px 14px;
    background: color-mix(in srgb, var(--accent) 13%, var(--bg-elevated));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 16%, transparent);
    color: var(--text-primary);
    font-size: var(--font-size-sm); line-height: 1.5;
    overflow-wrap: anywhere;
    user-select: text;
    white-space: pre-wrap;
  }
  .bubble strong { font-weight: 650; }
  code {
    font-family: var(--font-code); font-size: 0.92em;
    padding: 1px 5px; border-radius: 4px;
    background: var(--bg-overlay); color: var(--text-primary);
  }

  .assistant { display: flex; flex-direction: column; gap: 7px; min-width: 0; }
  .blocks { display: flex; flex-direction: column; gap: 8px; padding-left: 25px; min-width: 0; }

  /* What you can do with the answer, out of the way until the message is hovered or focused. */
  .hover {
    display: flex; align-items: center; gap: 1px; padding-left: 22px;
    opacity: 0; transform: translateY(-2px);
    transition: opacity var(--transition-fast), transform var(--transition-fast);
  }
  .assistant:hover .hover, .assistant:focus-within .hover { opacity: 1; transform: none; }
</style>
