<!--
  StudioViewSourceModal — shared "View implementation" modal.

  Used by RON / JSON / TOML studio modals to show the source of a
  schema type (Rust struct/enum source via `prettyplease`, or a
  JSON Schema fragment pretty-printed). Single component, single look:
  Prism-highlighted code with a gutter, copy button, and a status
  banner. Per-format extras (RON's reference-field gutter highlight)
  flow through the optional `decorateLine` callback.

  Why centralised: before this lived inline in each modal and only RON
  had syntax highlighting. The user pointed out the inconsistency —
  same UI, same component now.
-->
<script lang="ts" module>
  import type { TypeSource } from '$lib/ipc/studio-format';

  /** Source-language hint. Drives the Prism grammar pick + the gutter
   *  decoration (`rust` opts into per-line reference-field highlighting
   *  when `decorateLine` is provided). */
  export type SourceLanguage = 'rust' | 'json';

  /** Per-line decoration callback (RON-only today). Return the matched
   *  ref-field name for a line that should be gutter-highlighted, or
   *  `null` for plain lines. The host owns the ref-field convention. */
  export type LineDecorator = (rawLine: string) => string | null;

  export interface StudioViewSourceModalProps {
    /** The fetched source payload — `null` while loading or after an
     *  error. Renders the modal body's "fragment" view. */
    viewSource: TypeSource | null;
    /** True while `backend.schemaViewSource(...)` is in flight. */
    busy:       boolean;
    /** Error string from the last fetch. `null` clears the banner. */
    err:        string | null;
    /** Triggered on backdrop click, Escape, the close button, and the
     *  modal's own dismiss flow. Caller resets local state in response. */
    onClose:    () => void;
    /** Picks the Prism grammar. `rust` for RON (Rust struct/enum
     *  source) AND for TOML when bound to a Rust crate; `json` for
     *  JSON Schema fragments AND TOML bound to a JSON Schema. */
    language?:  SourceLanguage;
    /** RON-only — optional per-line ref-field highlighter. When
     *  provided, lines whose `decorateLine` returns non-null get a
     *  link icon in the gutter + an accent-tinted background, and
     *  a "N reference fields highlighted" banner appears at the top.
     *  Skipped silently for JSON-language sources. */
    decorateLine?: LineDecorator;
    /** Spinner label while loading. Defaults to "Loading source…". */
    loadingLabel?: string;
  }
</script>

<script lang="ts">
  import Modal from '../Modal.svelte';
  import StateBlock from '../ui/StateBlock.svelte';
  import Spinner from '../ui/Spinner.svelte';
  import { BookOpen, Copy, AlertCircle, Link as LinkIcon } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import Prism from 'prismjs';
  // Ensure prism-rust + prism-json are loaded (prism-shared imports both).
  import '$lib/utils/prism-shared';

  let {
    viewSource,
    busy,
    err,
    onClose,
    language = 'rust',
    decorateLine,
    loadingLabel = 'Loading source…',
  }: StudioViewSourceModalProps = $props();

  /** Decorated source split per line — empty when not RON-style. */
  const decorated = $derived.by<Array<{ html: string; refField: string | null }>>(() => {
    if (!viewSource) return [];
    const grammar = language === 'json'
      ? (Prism.languages.json ?? Prism.languages.javascript)
      : (Prism.languages.rust ?? Prism.languages.clike);
    const grammarName = language === 'json' ? 'json' : 'rust';
    const lines = viewSource.source.split('\n');
    return lines.map(line => ({
      html:     Prism.highlight(line === '' ? ' ' : line, grammar, grammarName),
      refField: language === 'rust' && decorateLine ? decorateLine(line) : null,
    }));
  });

  const refLineCount = $derived(decorated.filter(l => l.refField !== null).length);

  async function copyAll(): Promise<void> {
    if (!viewSource) return;
    await copyToClipboard(viewSource.source);
  }
</script>

<Modal
  onClose={onClose}
  width="min(820px, 92vw)"
  height="min(640px, 86vh)"
  padBody={false}
  ariaLabel="View implementation"
>
  {#snippet header()}
    <BookOpen size={14} class="vs-header-icon" />
    <span class="vs-title">
      {viewSource ? `${viewSource.name} · ${viewSource.kind}` : 'View implementation'}
    </span>
    {#if viewSource}
      <span class="vs-meta" use:tooltip={viewSource.canonical_path}>{viewSource.canonical_path}</span>
    {/if}
    <div class="vs-spacer"></div>
    {#if viewSource}
      <button type="button" class="vs-action-btn"
        onclick={copyAll}
        use:tooltip={'Copy source'}
      >
        <Copy size={13} /> <span>Copy</span>
      </button>
    {/if}
    <button class="mac-close-btn" onclick={onClose} aria-label="Close" use:tooltip={'Close'}></button>
  {/snippet}

  {#if busy}
    <StateBlock tone="loading">
      {#snippet spinner()}<Spinner size="lg" label={loadingLabel} />{/snippet}
    </StateBlock>
  {:else if err}
    <StateBlock tone="error" label={err}>
      {#snippet icon()}<AlertCircle size={16} />{/snippet}
    </StateBlock>
  {:else if viewSource}
    <div class="vs-source-wrap">
      {#if refLineCount > 0}
        <div class="vs-source-banner">
          <LinkIcon size={11} />
          <span>{refLineCount} reference field{refLineCount === 1 ? '' : 's'} highlighted</span>
        </div>
      {/if}
      <pre class="vs-source-pre language-{language}"><code class="language-{language}">{#each decorated as l, idx (idx)}<span class="vs-source-line" class:vs-source-line-ref={l.refField !== null}><span class="vs-source-gutter" aria-hidden="true">{#if l.refField}<LinkIcon size={10} />{/if}</span><span class="vs-source-code">{@html l.html}</span></span>{/each}</code></pre>
    </div>
  {/if}
</Modal>

<style>
  .vs-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .vs-meta {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-secondary);
    background: var(--bg-overlay);
    padding: 2px 8px;
    border-radius: 8px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 360px;
  }
  .vs-spacer { flex: 1; }
  :global(.vs-header-icon) { color: var(--accent); flex-shrink: 0; }

  .vs-action-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    border: 1px solid var(--border-subtle);
    background: var(--bg-base);
    color: var(--text-secondary);
    border-radius: var(--radius-sm);
    font-size: 11px;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .vs-action-btn:hover {
    color: var(--text-primary);
    background: var(--bg-overlay);
  }

  .vs-source-wrap {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }
  .vs-source-banner {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 14px;
    background: color-mix(in srgb, var(--accent) 10%, var(--bg-base));
    border-bottom: 1px solid var(--border-subtle);
    font-size: 11px;
    color: var(--accent);
    flex-shrink: 0;
  }
  .vs-source-pre {
    margin: 0;
    padding: 8px 0;
    height: auto;
    flex: 1;
    overflow: auto;
    background: var(--bg-base);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 12px;
    line-height: 1.55;
    white-space: pre;
  }
  .vs-source-pre > code {
    display: block;
    white-space: pre;
  }
  .vs-source-line {
    display: flex;
    align-items: flex-start;
    min-height: 1.55em;
  }
  .vs-source-line-ref {
    background: color-mix(in srgb, var(--accent) 7%, transparent);
  }
  .vs-source-gutter {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    flex-shrink: 0;
    color: var(--accent);
    opacity: 0.85;
  }
  .vs-source-code {
    flex: 1;
    min-width: 0;
    padding-right: 18px;
    white-space: pre;
  }
</style>
