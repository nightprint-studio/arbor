<script lang="ts">
  /**
   * A Word document, in an editor tab.
   *
   * ## Why a renderer and not a converter
   *
   * The cheap answer is to convert the document to semantic HTML (`mammoth`) and style it
   * like a Markdown preview. It reads fine and it answers the wrong question: the reason
   * you open a `.docx` from a project tree is almost always to check *the document* — a
   * spec, a hand-off, a table somebody sent — and a version with the layout thrown away
   * cannot be checked against the one the sender is looking at. `docx-preview` renders the
   * pages, the styles, the tables and the images as Word laid them out.
   *
   * ## How the bytes get here
   *
   * Over IPC as base64, not through the asset protocol. `BennuImageView` uses the asset URL
   * because an `<img>` streams it and a 4 MB texture would otherwise cross the seam twice —
   * but that protocol's scope lists the media types it serves, a `.docx` is not one of
   * them, and widening it to serve arbitrary documents to the WebView buys a copy we do not
   * need: a document is tens of kilobytes, and this has to end up as an ArrayBuffer anyway.
   *
   * ## Read-only, and it says so
   *
   * Nothing here edits. The file never enters the source cache (see `opensAsPreview`), so
   * there is no buffer for a stray Ctrl+S to write back over the document.
   */
  import { FileText, ExternalLink } from 'lucide-svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { fsReadBytes } from '$lib/ipc/fs';
  import { openPath } from '@tauri-apps/plugin-opener';
  import { baseName } from '$lib/utils/paths';
  import { formatBytes } from '$lib/utils/format';
  import { tooltip } from '$lib/actions/tooltip';

  let { path }: { path: string } = $props();

  let host = $state<HTMLDivElement | null>(null);
  let loading = $state(true);
  let error = $state('');
  let bytes = $state(0);

  /** Decode the base64 the IPC seam hands back. Done in one pass over the binary string
   *  rather than through `fetch(data:)`, which would copy it a third time. */
  function toBytes(b64: string): Uint8Array {
    const bin = atob(b64);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i += 1) out[i] = bin.charCodeAt(i);
    return out;
  }

  $effect(() => {
    const file = path;
    const container = host;
    if (!container) return;
    let cancelled = false;

    loading = true;
    error = '';
    container.replaceChildren();

    void (async () => {
      try {
        const data = toBytes(await fsReadBytes(file));
        // Imported here and not at the top: the renderer pulls in a ZIP reader and its own
        // layout engine, and a project full of `.java` should not pay for that at startup.
        const { renderAsync } = await import('docx-preview');
        if (cancelled) return;
        bytes = data.byteLength;
        await renderAsync(data, container, undefined, {
          className: 'docx',
          inWrapper: true,
          ignoreWidth: false,
          ignoreHeight: false,
          breakPages: true,
          experimental: true,
          // The document's own fonts are not installed here, and substituting them silently
          // is what makes a rendered page disagree with the sender's. Letting the renderer
          // apply what it finds keeps the disagreement visible rather than plausible.
          useBase64URL: true,
        });
        if (!cancelled) loading = false;
      } catch (e) {
        if (cancelled) return;
        error = e instanceof Error ? e.message : String(e);
        loading = false;
      }
    })();

    return () => { cancelled = true; };
  });
</script>

<div class="dv">
  <div class="dv-bar">
    <FileText size={13} />
    <span class="dv-name">{baseName(path)}</span>
    {#if bytes}<span class="dv-meta">{formatBytes(bytes)}</span>{/if}
    <span class="dv-ro">read-only</span>
    <button
      class="dv-open"
      type="button"
      use:tooltip={'Open in the system application'}
      aria-label="Open in the system application"
      onclick={() => void openPath(path).catch(() => {})}
    >
      <ExternalLink size={13} />
    </button>
  </div>

  {#if error}
    <div class="dv-state">
      <EmptyState
        message="This document could not be rendered."
        description={error}
      />
    </div>
  {:else if loading}
    <div class="dv-state"><Spinner size="lg" label="Rendering…" /></div>
  {/if}

  <!-- Always mounted: the renderer writes into it, and creating it only after the load
       would mean the effect has nowhere to render into on the first pass. -->
  <div class="dv-page" class:hidden={loading || !!error} bind:this={host}></div>
</div>

<style>
  .dv { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--bg-base); }

  .dv-bar {
    display: flex; align-items: center; gap: 8px; flex: none;
    height: 28px; padding: 0 10px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs); color: var(--text-muted);
  }
  .dv-name { color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dv-meta { color: var(--text-faint); }
  .dv-ro {
    margin-left: auto;
    font-size: var(--font-size-2xs); text-transform: uppercase; letter-spacing: 0.05em;
    color: var(--text-faint);
  }
  .dv-open {
    display: inline-flex; align-items: center; justify-content: center;
    width: 20px; height: 20px; padding: 0;
    background: none; border: 0; border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer;
  }
  .dv-open:hover { background: var(--bg-hover); color: var(--text-primary); }

  .dv-state { flex: 1; display: flex; align-items: center; justify-content: center; min-height: 0; }

  /* The scroll surface. A rendered page is a white sheet whatever the app's theme is —
     that is what the document IS — so the surround is deliberately neutral and darker,
     the way every document viewer frames a page. */
  .dv-page {
    flex: 1; min-height: 0; overflow: auto;
    padding: 16px;
    background: var(--bg-elevated);
  }
  .dv-page.hidden { display: none; }

  /* The renderer emits its own markup and its own class names; these are the only two
     things the app has an opinion about — that a page reads as a sheet, and that it is
     centred in whatever width the panel happens to be. */
  .dv-page :global(.docx-wrapper) { background: transparent; padding: 0; display: flex; flex-direction: column; align-items: center; gap: 14px; }
  .dv-page :global(.docx) { box-shadow: 0 4px 18px rgba(0, 0, 0, 0.45); }
</style>
