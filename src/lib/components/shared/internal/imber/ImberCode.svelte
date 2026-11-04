<script lang="ts">
  /**
   * A block of code in an answer: the language named in a quiet header, a copy button that appears on
   * hover, and the code highlighted by the same highlighter the docs use. Scrolls sideways rather than
   * wrapping — a wrapped line of code stops being readable as code.
   */
  import { Copy } from 'lucide-svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { highlightCode } from '$lib/utils/highlight';

  let { language, code }: { language: string; code: string } = $props();
</script>

<figure class="code">
  <figcaption>
    <span class="lang">{language}</span>
    <IconButton tooltip="Copy" size={20} onclick={() => copyToClipboard(code, { successToast: 'Copied' })}>
      <Copy size={11} />
    </IconButton>
  </figcaption>
  <pre><code>{@html highlightCode(code, language)}</code></pre>
</figure>

<style>
  .code {
    margin: 0;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
    overflow: hidden;
  }
  figcaption {
    display: flex; align-items: center; justify-content: space-between;
    padding: 3px 4px 3px 10px;
    border-bottom: 1px solid var(--border-subtle);
    background: color-mix(in srgb, var(--bg-overlay) 50%, transparent);
  }
  .lang { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); text-transform: lowercase; }
  figcaption :global(button) { opacity: 0; transition: opacity var(--transition-fast); }
  .code:hover figcaption :global(button) { opacity: 1; }
  pre {
    margin: 0; padding: 9px 12px; overflow-x: auto;
    font-family: var(--font-code); font-size: var(--font-size-xs); line-height: 1.55;
    color: var(--text-primary);
  }
  code { font-family: inherit; }
</style>
