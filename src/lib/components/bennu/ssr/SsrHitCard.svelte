<script lang="ts">
  /**
   * What the hover preview of a result says.
   *
   * **Searching:** the code around the match, a few lines either side, the match itself marked.
   * **Replacing:** the rewrite of that match as the replacement preview computed it — the changed lines with
   * their neighbours, before and after — or, until the preview has been built, the code around it and a line
   * saying so. The text is read lazily by the owner and arrives as a prop; nothing here touches the disk.
   */
  import { bennuSsrStore as store } from '$lib/stores/bennu/ssr.svelte';
  import type { SsrHit } from '$lib/ipc/bennu/ssr';
  import { contextAround, hunkFor } from './ssr-context';
  import { hunksOf } from './ssr-diff';

  let { hit, text }: { hit: SsrHit; text: string | null } = $props();

  const context = $derived(text === null ? null : contextAround(text, hit.range, 3));
  const previewed = $derived(store.preview?.files.find((f) => f.file === hit.file) ?? null);
  const hunk = $derived.by(() => {
    if (!store.replacing || !previewed || !context) return null;
    return hunkFor(hunksOf(previewed.before, previewed.after, 3), context.startLine, context.endLine);
  });
</script>

<div class="hc">
  <div class="hc-head">
    <span class="hc-where">{hit.rel}:{hit.line}</span>
    {#if hit.enclosing}<span class="hc-in">in {hit.enclosing}</span>{/if}
  </div>

  {#if context === null}
    <p class="hc-note">Reading…</p>
  {:else if hunk}
    <div class="hc-code">
      {#each hunk.lines as line, i (i)}
        <div class="hc-line hc-{line.kind}">
          <span class="hc-no">{line.oldNo ?? ''}</span>
          <span class="hc-sign">{line.kind === 'add' ? '+' : line.kind === 'del' ? '−' : ' '}</span>
          <span class="hc-text">{line.text}</span>
        </div>
      {/each}
    </div>
  {:else}
    <div class="hc-code">
      {#each context.lines as line (line.no)}
        <div class="hc-line" class:hc-on={line.from !== undefined}>
          <span class="hc-no">{line.no}</span>
          <span class="hc-sign"></span>
          <span class="hc-text">{#if line.from !== undefined && line.to !== undefined}{line.text.slice(0, line.from)}<mark>{line.text.slice(line.from, line.to)}</mark>{line.text.slice(line.to)}{:else}{line.text}{/if}</span>
        </div>
      {/each}
      {#if context.cut}<div class="hc-line"><span class="hc-no"></span><span class="hc-sign"></span><span class="hc-text hc-dim">…</span></div>{/if}
    </div>
    {#if store.replacing}
      <p class="hc-note">
        {previewed ? 'This match has no changed lines in the preview.' : 'Preview changes (Ctrl+Shift+Enter) to see what this becomes.'}
      </p>
    {/if}
  {/if}
</div>

<style>
  .hc { display: flex; flex-direction: column; }
  .hc-head {
    display: flex; align-items: baseline; gap: 10px; padding: 5px 10px;
    border-bottom: 1px solid var(--border-subtle);
    font-family: var(--font-code); font-size: var(--font-size-2xs);
  }
  .hc-where { color: var(--text-secondary); }
  .hc-in { color: var(--text-disabled); }
  .hc-code { font-family: var(--font-code); font-size: var(--font-size-2xs); line-height: 1.55; padding: 4px 0; overflow: hidden; }
  .hc-line { display: flex; white-space: pre; color: var(--text-secondary); }
  .hc-on { background: color-mix(in srgb, var(--accent) 7%, transparent); color: var(--text-primary); }
  .hc-no { flex-shrink: 0; width: 38px; padding-right: 8px; text-align: right; color: var(--text-disabled); }
  .hc-sign { flex-shrink: 0; width: 14px; text-align: center; }
  .hc-text { min-width: 0; padding-right: 10px; overflow: hidden; text-overflow: ellipsis; }
  .hc-dim { color: var(--text-disabled); }
  mark {
    color: inherit; border-radius: 2px;
    background: color-mix(in srgb, var(--accent) 24%, transparent);
    text-decoration: underline; text-decoration-color: var(--accent); text-underline-offset: 2px;
  }
  .hc-add { background: color-mix(in srgb, var(--success) 14%, transparent); }
  .hc-add .hc-sign { color: var(--success); }
  .hc-del { background: color-mix(in srgb, var(--error) 14%, transparent); }
  .hc-del .hc-sign { color: var(--error); }
  .hc-note { margin: 0; padding: 5px 10px; border-top: 1px solid var(--border-subtle); font-size: var(--font-size-3xs); color: var(--text-muted); }
</style>
