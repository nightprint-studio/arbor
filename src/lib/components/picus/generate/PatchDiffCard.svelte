<script lang="ts">
  /**
   * One destination's proposed patch, as a reviewable diff.
   *
   * What is shown is **what `picus_preview_apply` returned** — the file before and
   * the file after, computed by the same code that would do the write. Nothing
   * here re-derives the placement from the target's role: a card that guesses
   * where the block will land is a card that can be wrong about it, and being
   * right about that is the entire reason the preview exists.
   *
   * The hunk header states the insertion rule in the backend's own words
   * (`reasons`), because a predictable rule you can read beats a clever one you
   * cannot: knowing where the block lands is half of trusting the write.
   *
   * Encoding and line endings of the destination are shown alongside: the patch is
   * applied to the original bytes, so a windows-1252 file stays windows-1252 and a
   * CRLF file stays CRLF.
   */
  import { ChevronRight, FileCode2, FilePlus2 } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import PicusRoleChip from '../PicusRoleChip.svelte';
  import EncodingPill from '$lib/components/shared/internal/EncodingPill.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { spliceDiff } from '$lib/utils/picus/line-diff';
  import type { PreviewFile } from '$lib/ipc/picus/scripts';
  import type { Target } from '$lib/types/picus';

  interface Props {
    file: PreviewFile;
    /** The destination this file belongs to, when one is known — for its chips. */
    target?: Target | null;
  }

  let { file, target = null }: Props = $props();

  let open = $state(true);

  const diff = $derived(spliceDiff(file.before, file.after));
  const entry = $derived(picusProjectStore.fileByPath(file.path));
  /** The expected encoding is a project fact, so it comes off the tree, not the preview. */
  const expected = $derived(entry?.expectedEncoding);
</script>

<div class="pd" class:pd-noop={diff.unchanged}>
  <button class="pd-head" aria-expanded={open} onclick={() => (open = !open)}>
    <span class="pd-twist" class:pd-open={open}><ChevronRight size={13} /></span>
    {#if file.createsFile}<FilePlus2 size={13} />{:else}<FileCode2 size={13} />{/if}
    <span class="pd-path">{file.path}</span>
    {#if target}
      <PicusDialectChip engine={target.dialect} terse />
      <PicusRoleChip role={target.role} terse />
    {/if}
    <EncodingPill encoding={file.encoding} {expected} eol={file.eol} compact />
    <span class="pd-spacer"></span>
    {#if file.createsFile}
      <Badge variant="tone" tone="accent" size="sm" label="new file" />
    {/if}
    {#if diff.unchanged}
      <Badge variant="tone" tone="neutral" size="sm" label="no change" />
    {:else}
      {#if diff.removed.length}
        <Badge variant="tone" tone="error" size="sm" label={`−${diff.removed.length}`} />
      {/if}
      <Badge variant="tone" tone="success" size="sm" label={`+${diff.added.length}`} />
    {/if}
  </button>

  {#if open}
    <div class="pd-diff" role="region" aria-label={`Patch for ${file.path}`}>
      <div class="pd-line pd-hunk">
        <span class="pd-num"></span>
        <span class="pd-sign"></span>
        <span class="pd-text">
          @@ line {diff.startLine} · {diff.unchanged
            ? 'nothing to write'
            : `−${diff.removed.length} +${diff.added.length}`} @@
        </span>
      </div>

      {#each diff.contextBefore as line, i (`b${i}`)}
        <div class="pd-line pd-ctx">
          <span class="pd-num">{diff.startLine - diff.contextBefore.length + i}</span>
          <span class="pd-sign"></span>
          <span class="pd-text">{line}</span>
        </div>
      {/each}

      {#each diff.removed as line, i (`r${i}`)}
        <div class="pd-line pd-del">
          <span class="pd-num">{diff.startLine + i}</span>
          <span class="pd-sign">−</span>
          <span class="pd-text">{line}</span>
        </div>
      {/each}

      {#each diff.added as line, i (`a${i}`)}
        <div class="pd-line pd-add">
          <span class="pd-num"></span>
          <span class="pd-sign">+</span>
          <span class="pd-text">{line}</span>
        </div>
      {/each}

      {#each diff.contextAfter as line, i (`f${i}`)}
        <div class="pd-line pd-ctx">
          <span class="pd-num">{diff.startLine + diff.removed.length + i}</span>
          <span class="pd-sign"></span>
          <span class="pd-text">{line}</span>
        </div>
      {/each}
    </div>

    {#if file.reasons.length}
      <ul class="pd-reasons">
        {#each file.reasons as reason, i (i)}
          <li>{reason}</li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>

<style>
  .pd {
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--bg-base);
  }
  /* A destination the write would leave untouched is not a change — say so quietly
     rather than showing an empty diff that looks like a bug. */
  .pd-noop { opacity: 0.72; }

  .pd-head {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 10px;
    background: var(--bg-elevated);
    border: none;
    border-bottom: 1px solid var(--border-subtle);
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .pd-head:hover { background: var(--bg-hover); }
  .pd-spacer { flex: 1; }

  .pd-twist {
    display: inline-flex;
    color: var(--text-disabled);
    transition: transform var(--transition-fast);
  }
  .pd-twist.pd-open { transform: rotate(90deg); }

  .pd-path {
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 380px;
  }

  .pd-diff {
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    line-height: 1.65;
    overflow-x: auto;
  }
  .pd-line { display: flex; white-space: pre; min-width: max-content; }
  .pd-num {
    width: 38px;
    flex-shrink: 0;
    text-align: right;
    padding-right: 10px;
    color: var(--text-disabled);
    user-select: none;
  }
  .pd-sign { width: 14px; flex-shrink: 0; color: var(--text-disabled); }
  .pd-text { padding-right: 12px; }

  .pd-add { background: var(--diff-add-bg); }
  .pd-add .pd-sign { color: var(--success); }
  .pd-del { background: var(--diff-del-bg); }
  .pd-del .pd-sign { color: var(--error); }
  .pd-ctx { color: var(--text-muted); }
  .pd-hunk { background: var(--info-subtle); color: var(--info); }

  /* Why the block lands where it lands — the backend's words, not a guess. */
  .pd-reasons {
    margin: 0;
    padding: 7px 12px 8px 30px;
    border-top: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs);
    line-height: 1.55;
    color: var(--text-muted);
  }
</style>
