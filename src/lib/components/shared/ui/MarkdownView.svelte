<script lang="ts">
  /**
   * MarkdownView — read-only rendered Markdown.
   *
   * Headings, lists, tables, quotes, links, bold and italic, inline code and fenced code with syntax
   * highlighting, through the app's one sanitising renderer (`renderMarkdown`): raw HTML that could run is
   * stripped, links are shown but never followed. Text stays selectable, because the first thing anyone does
   * with a rendered answer is copy part of it.
   *
   * `streaming` is for text that is still arriving: a code fence that has been opened and not yet closed
   * is closed for the render, so a half-written block shows as code instead of as stray backticks that
   * snap into a block when the closing fence finally lands.
   */
  import { mount, unmount, untrack } from 'svelte';
  import MermaidDiagram from './MermaidDiagram.svelte';
  import { MERMAID_MAX_SOURCE } from '$lib/utils/mermaid';
  import { renderMarkdown } from '$lib/utils/markdown';
  import { copyToClipboard } from '$lib/utils/clipboard';

  let { text, streaming = false }: { text: string; streaming?: boolean } = $props();

  /** Close a fence the text has opened and not finished. */
  function closeOpenFence(md: string): string {
    const fences = (md.match(/^```/gm) ?? []).length;
    return fences % 2 === 1 ? `${md}\n\`\`\`` : md;
  }

  const html = $derived(renderMarkdown(streaming ? closeOpenFence(text) : text));

  let host = $state<HTMLElement | null>(null);

  // A ```mermaid fence is drawn, not printed. The diagram is a component mounted over the rendered code
  // block, so the library is fetched only when a fence actually appears (see `utils/mermaid.ts`). A fence
  // still being written is left as code: half a diagram is only an error. Declared before the copy-button
  // effect so a diagram never grows a "Copy" of its own on top of the diagram's.
  $effect(() => {
    void html;
    if (!host) return;
    const open = untrack(() => streaming) && (text.match(/^```/gm) ?? []).length % 2 === 1;
    const all = host.querySelectorAll<HTMLElement>('pre.md-pre');
    const last = all[all.length - 1];
    const mounted: Record<string, unknown>[] = [];
    for (const pre of host.querySelectorAll<HTMLElement>('pre.md-pre[data-lang="mermaid"]')) {
      const source = pre.querySelector('code')?.textContent ?? '';
      if ((open && pre === last) || source.length > MERMAID_MAX_SOURCE) continue;
      const slot = document.createElement('div');
      pre.replaceWith(slot);
      mounted.push(mount(MermaidDiagram, { target: slot, props: { source } }));
    }
    return () => { for (const m of mounted) void unmount(m); };
  });

  // A copy button on every code block. Added to the rendered DOM rather than to the renderer's output, so
  // the other places that render Markdown do not grow a button they have no style for.
  $effect(() => {
    void html;
    if (!host) return;
    for (const pre of host.querySelectorAll<HTMLElement>('pre.md-pre')) {
      if (pre.querySelector('.md-copy')) continue;
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'md-copy';
      button.textContent = 'Copy';
      button.addEventListener('click', () => copyToClipboard(pre.querySelector('code')?.textContent ?? '', { successToast: 'Copied' }));
      pre.appendChild(button);
    }
  });
</script>

<div class="md-view" bind:this={host}>{@html html}</div>

<style>
  .md-view { font-size: var(--font-size-md); line-height: 1.6; color: var(--text-primary); overflow-wrap: anywhere; user-select: text; }
  .md-view :global(*) { user-select: text; }

  .md-view :global(.md-p) { margin: 0 0 8px; }
  .md-view :global(.md-p:last-child) { margin-bottom: 0; }
  .md-view :global(.md-spacer) { height: 2px; }
  .md-view :global(strong) { font-weight: 650; color: var(--text-primary); }
  .md-view :global(em) { color: var(--text-secondary); }

  /* Headings carry the accent: they are what the eye should find first in a long answer. */
  .md-view :global(.md-h1) { font-size: var(--font-size-lg); font-weight: 700; margin: 14px 0 6px; display: block; color: var(--accent); }
  .md-view :global(.md-h2) { font-size: var(--font-size-md); font-weight: 650; margin: 12px 0 5px; display: block; padding-bottom: 3px; color: var(--accent); border-bottom: 1px solid color-mix(in srgb, var(--accent) 30%, var(--border-subtle)); }
  .md-view :global(.md-h3) { font-size: var(--font-size-md); font-weight: 600; margin: 10px 0 4px; display: block; color: var(--info); }
  .md-view :global(.md-h1:first-child), .md-view :global(.md-h2:first-child), .md-view :global(.md-h3:first-child) { margin-top: 0; }

  .md-view :global(.md-ul), .md-view :global(.md-ol) { margin: 4px 0 8px; padding-left: 20px; }
  .md-view :global(.md-ul li), .md-view :global(.md-ol li) { margin: 2px 0; }
  .md-view :global(.md-ul li::marker), .md-view :global(.md-ol li::marker) { color: var(--accent); }

  .md-view :global(.md-bq) {
    margin: 6px 0; padding: 4px 11px;
    border-left: 3px solid var(--accent); border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    background: var(--bg-overlay); color: var(--text-secondary);
  }
  .md-view :global(.md-hr) { border: none; border-top: 1px solid var(--border-subtle); margin: 10px 0; }
  .md-view :global(.md-link) { color: var(--accent); }

  /* An inline code span is tinted by what it names, with the colours the editor gives that thing: an
     annotation, a call, a type, a constant, a file. It wraps (a long span must not push the line wide) and
     has no border, which on a span that breaks across lines draws a box around each fragment. */
  .md-view :global(.md-inline-code) {
    --ic: var(--text-primary);
    font-family: var(--font-code); font-size: 0.88em;
    padding: 1px 5px; border-radius: var(--radius-sm);
    color: var(--ic);
    background: color-mix(in srgb, var(--ic) 11%, var(--bg-overlay));
    -webkit-box-decoration-break: clone; box-decoration-break: clone;
    overflow-wrap: anywhere;
  }
  .md-view :global(.md-ic-annotation) { --ic: var(--syntax-annotation, #bbb529); }
  .md-view :global(.md-ic-call)       { --ic: var(--syntax-function, #ffc66d); }
  .md-view :global(.md-ic-type)       { --ic: var(--syntax-type, #4ec9b0); }
  .md-view :global(.md-ic-const)      { --ic: var(--syntax-number, #9876aa); }
  .md-view :global(.md-ic-number)     { --ic: var(--syntax-number, #6897bb); }
  .md-view :global(.md-ic-keyword)    { --ic: var(--syntax-keyword, #cc7832); }
  .md-view :global(.md-ic-ident)      { --ic: var(--syntax-field, #9876aa); }
  .md-view :global(.md-ic-path)       { --ic: var(--info); }
  .md-view :global(.md-pre) {
    position: relative;
    margin: 8px 0; padding: 10px 12px; overflow-x: auto;
    border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
    background: var(--bg-base);
    font-family: var(--font-code); font-size: var(--font-size-xs); line-height: 1.55;
  }
  /* The language, quietly, in the corner; and Copy, which shows when the block is hovered. */
  .md-view :global(.md-pre[data-lang]:not([data-lang=""]))::before {
    content: attr(data-lang); display: block; margin-bottom: 6px;
    font-family: var(--font-ui); font-size: 10px; letter-spacing: 0.05em; text-transform: uppercase; color: var(--text-muted);
  }
  .md-view :global(.md-copy) {
    position: absolute; top: 6px; right: 8px;
    padding: 1px 7px; border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
    background: var(--bg-elevated); color: var(--text-secondary);
    font-family: var(--font-ui); font-size: 10px; cursor: pointer;
    opacity: 0; transition: opacity var(--transition-fast);
  }
  .md-view :global(.md-pre:hover .md-copy), .md-view :global(.md-copy:focus-visible) { opacity: 1; }
  .md-view :global(.md-copy:hover) { color: var(--text-primary); border-color: var(--accent); }
  .md-view :global(.md-code) { font-family: var(--font-code); font-size: inherit; background: none; padding: 0; }

  .md-view :global(.md-table-wrap) { margin: 8px 0; overflow-x: auto; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); }
  .md-view :global(.md-table) { width: 100%; border-collapse: collapse; font-size: var(--font-size-sm); }
  .md-view :global(.md-table th), .md-view :global(.md-table td) { padding: 5px 10px; text-align: left; border-bottom: 1px solid var(--border-subtle); }
  .md-view :global(.md-table th) { font-weight: 600; background: var(--bg-overlay); white-space: nowrap; }
  .md-view :global(.md-table tr:last-child td) { border-bottom: none; }
</style>
