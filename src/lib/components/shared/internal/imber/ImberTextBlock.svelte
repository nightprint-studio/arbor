<script lang="ts">
  /**
   * A passage of Imber's answer as a surface of its own at full contrast — not a grey paragraph between
   * receipts. Line breaks are kept (a model writes in paragraphs and lists); the typewriter paces text that
   * is written while you watch, and a caret follows the last passage of a live message.
   */
  import ImberText from './ImberText.svelte';
  import './imber-motion.css';

  let { text, live = false, caret = false }: {
    text: string;
    /** The run that writes it is still going. */
    live?: boolean;
    /** Show the blinking caret after the text (the last passage of a live message). */
    caret?: boolean;
  } = $props();
</script>

<div class="text"><ImberText {text} {live} />{#if caret}<span class="asst-caret" aria-hidden="true"></span>{/if}</div>

<style>
  .text {
    margin: 0;
    padding: 10px 13px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
    font-size: var(--font-size-md); line-height: 1.6;
    color: var(--text-primary);
    overflow-wrap: anywhere;
    /* Set by the thread when the panel is expanded: prose keeps a readable line, code stays wide. */
    max-width: var(--asst-text-max, none);
  }
</style>
