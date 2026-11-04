<script lang="ts">
  /**
   * The formatter's live preview — a representative Java snippet, formatted with the style being
   * edited and shown read-only in the real editor (so it is coloured exactly like the buffer).
   */
  import { Eye } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import CodePreview from '$lib/components/shared/ui/CodePreview.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { languageForPath } from '../languages';

  interface Props {
    /** The formatted snippet; empty until the first answer lands. */
    text: string;
    /** Why the last attempt failed, if it did. */
    error: string | null;
  }

  let { text, error }: Props = $props();

  const language = languageForPath('Preview.java');
</script>

<section class="fp" aria-label="Formatter preview">
  <div class="card-section-title"><Eye size={12} /> Preview</div>
  {#if error}
    <Alert variant="error" compact text={`The preview could not be produced: ${error}`} />
  {/if}
  <div class="fp-code">
    {#if text}
      <CodePreview {text} {language} />
    {:else if !error}
      <div class="fp-wait"><Spinner block label="Formatting…" /></div>
    {/if}
  </div>
</section>

<style>
  .fp { display: flex; flex-direction: column; gap: 8px; min-height: 0; height: 100%; }
  .fp-code {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--bg-base);
  }
  .fp-wait { height: 100%; display: flex; align-items: center; justify-content: center; }
</style>
