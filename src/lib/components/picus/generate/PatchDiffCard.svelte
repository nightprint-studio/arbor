<script lang="ts">
  /**
   * One destination's proposed patch, as a reviewable diff.
   *
   * Nothing is written until this has been seen. The hunk header states the
   * **insertion rule** in words — "appended at the end of the file", "after the
   * last statement on this table" — because a predictable dumb rule you can read
   * beats a clever one you cannot: knowing where the block will land is half of
   * trusting the write.
   *
   * Encoding and line endings of the destination are shown alongside: the patch
   * is applied to the original bytes, so a windows-1252 file stays
   * windows-1252 and a CRLF file stays CRLF.
   */
  import { ChevronRight, FileCode2 } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import PicusRoleChip from '../PicusRoleChip.svelte';
  import EncodingPill from '$lib/components/shared/internal/EncodingPill.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import type { Target } from '$lib/types/picus';

  interface Props {
    target: Target;
    sql: string;
  }

  let { target, sql }: Props = $props();

  let open = $state(true);

  const lines = $derived(sql.split('\n'));
  const file = $derived(picusProjectStore.fileByPath(target.file));

  /** The declared insertion rule for this destination's role. */
  const placement = $derived(
    target.role === 'update'
      ? 'appended at the end of the file'
      : 'after the last statement touching this table',
  );
</script>

<div class="pd">
  <button class="pd-head" aria-expanded={open} onclick={() => (open = !open)}>
    <span class="pd-twist" class:pd-open={open}><ChevronRight size={13} /></span>
    <FileCode2 size={13} />
    <span class="pd-path">{target.file}</span>
    <PicusDialectChip dialect={target.dialect} terse />
    <PicusRoleChip role={target.role} terse />
    {#if file}
      <EncodingPill encoding={file.encoding} expected={file.expectedEncoding} eol={file.eol} compact />
    {/if}
    <span class="pd-spacer"></span>
    <Badge variant="tone" tone="success" size="sm" label={`+${lines.length}`} />
  </button>

  {#if open}
    <div class="pd-diff" role="region" aria-label={`Patch for ${target.file}`}>
      <div class="pd-line pd-hunk">
        <span class="pd-num"></span>
        <span class="pd-sign"></span>
        <span class="pd-text">@@ {placement} · +{lines.length} lines @@</span>
      </div>
      {#each lines as line, i (i)}
        <div class="pd-line pd-add">
          <span class="pd-num"></span>
          <span class="pd-sign">+</span>
          <span class="pd-text">{line}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .pd {
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--bg-base);
  }

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
    font-size: 11.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 380px;
  }

  .pd-diff {
    font-family: var(--font-code);
    font-size: 11.5px;
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
  .pd-hunk { background: var(--info-subtle); color: var(--info); }
</style>
