<script lang="ts">
  /**
   * MermaidDiagram — a Mermaid source drawn as a picture.
   *
   * The library is fetched the first time any diagram is shown (see `utils/mermaid.ts`) and the drawing is
   * asynchronous, so a diagram never holds the screen: until it is ready the box says so, and once it is
   * the picture is capped to a sensible height and can be opened larger. A source that does not parse is
   * shown as text under mermaid's own message — which names the line — rather than as a blank hole.
   * Copy and "open larger" are buttons, so both are reachable by Tab.
   */
  import { Copy, Maximize2, Code } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { cachedMermaid, mermaidErrorMessage, renderMermaid } from '$lib/utils/mermaid';

  let { source, title = '' }: {
    source: string;
    /** A caption above the picture. */
    title?: string;
  } = $props();

  // Drawn already (the text around it was re-rendered, or it was shown before): no flash of "drawing".
  let svg = $state<string | null>(null);
  let error = $state<string | null>(null);
  let large = $state(false);

  $effect(() => {
    const code = source;
    const hit = cachedMermaid(code);
    svg = hit;
    error = null;
    if (hit) return;
    let current = true;
    renderMermaid(code)
      .then((out) => { if (current) svg = out; })
      .catch((e) => { if (current) error = mermaidErrorMessage(e); });
    return () => { current = false; };
  });

  const copy = () => copyToClipboard(source, { successToast: 'Copied the diagram source' });
</script>

<figure class="mmd">
  <figcaption>
    <span class="cap">{title || 'Diagram'}</span>
    <span class="tools">
      <IconButton tooltip="Copy the Mermaid source" size={22} onclick={copy}><Copy size={12} /></IconButton>
      {#if svg}
        <IconButton tooltip="Open larger" size={22} onclick={() => (large = true)}><Maximize2 size={12} /></IconButton>
      {/if}
    </span>
  </figcaption>

  {#if error}
    <div class="failed">
      <Alert variant="error" text={error} compact />
      <pre class="src"><code>{source}</code></pre>
    </div>
  {:else if svg}
    <div class="pic">{@html svg}</div>
  {:else}
    <div class="wait"><Spinner size={14} /><span>Drawing the diagram…</span></div>
  {/if}
</figure>

{#if large && svg}
  <Modal onClose={() => (large = false)} width="min(1100px, 92vw)" height="min(760px, 88vh)" padBody={false} ariaLabel={title || 'Diagram'}>
    {#snippet header()}
      <ModalHeader onClose={() => (large = false)}>
        <Code size={14} />
        <span class="mtitle">{title || 'Diagram'}</span>
      </ModalHeader>
    {/snippet}
    <div class="big">{@html svg}</div>
  </Modal>
{/if}

<style>
  .mmd {
    margin: 8px 0; padding: 0;
    border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
    background: var(--bg-base); overflow: hidden;
  }
  figcaption {
    display: flex; align-items: center; justify-content: space-between;
    padding: 3px 4px 3px 10px;
    border-bottom: 1px solid var(--border-subtle);
    background: color-mix(in srgb, var(--bg-overlay) 50%, transparent);
  }
  .cap { font-size: var(--font-size-2xs); color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.05em; }
  .tools { display: flex; gap: 1px; }
  /* Capped: a tall diagram scrolls inside its box instead of pushing the answer off the screen. */
  .pic { padding: 12px; max-height: 420px; overflow: auto; display: flex; justify-content: center; }
  .pic :global(svg) { max-width: 100%; height: auto; }
  .wait { display: flex; align-items: center; gap: 8px; padding: 14px 12px; font-size: var(--font-size-xs); color: var(--text-muted); }
  .failed { display: flex; flex-direction: column; gap: 6px; padding: 8px; }
  .src {
    margin: 0; padding: 8px 10px; overflow: auto; max-height: 260px;
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm); background: var(--bg-elevated);
    font-family: var(--font-code); font-size: var(--font-size-xs); line-height: 1.55; color: var(--text-primary);
  }
  .src code { font-family: inherit; }
  .mtitle { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .big { height: 100%; overflow: auto; padding: 18px; display: flex; justify-content: center; align-items: flex-start; background: var(--bg-base); }
  .big :global(svg) { max-width: none; height: auto; min-width: 60%; }
</style>
