<script lang="ts">
  /**
   * EncodingConvertPlanList — the dry run of an encoding conversion, one row per file that is not
   * already right: what happens to it, and — for the files the target cannot fully hold — which
   * characters, and where.
   */
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { ConvertPlan, FilePlan, FileStatus } from '$lib/ipc/bennu/encoding-convert';
  import { formatBytes } from './encodings';

  let { plan, fallback }: { plan: ConvertPlan; fallback: 'abort' | 'replace' | 'escape' } = $props();

  type Tone = 'success' | 'warning' | 'error' | 'neutral';
  const STATUS: Record<FileStatus, { label: string; tone: Tone }> = {
    convert: { label: 'Convert', tone: 'success' },
    unchanged: { label: 'Already right', tone: 'neutral' },
    binary: { label: 'Binary', tone: 'neutral' },
    invalid: { label: 'Not valid', tone: 'error' },
    blocked: { label: 'Blocked', tone: 'error' },
    unreadable: { label: 'Unreadable', tone: 'error' },
    'unknown-encoding': { label: 'Unknown encoding', tone: 'warning' },
  };

  /** What happens to the characters that do not fit, in the words the person chose. */
  const fate = $derived(
    fallback === 'abort' ? 'stop the conversion'
      : fallback === 'replace' ? 'become “?”'
        : 'be written as escapes',
  );

  function bomNote(f: FilePlan): string | null {
    if (f.bom_before === f.bom_after) return null;
    return f.bom_after ? 'adds a BOM' : 'drops the BOM';
  }
</script>

<ul class="plan" aria-label="Files in the conversion">
  {#each plan.files as f (f.file)}
    {@const s = STATUS[f.status]}
    <li class="row">
      <div class="head">
        <Badge variant="tone" tone={s.tone} size="sm" label={s.label} />
        <span class="path" use:tooltip={f.file}>{f.rel}</span>
        {#if f.status === 'convert'}
          <span class="meta">{f.from} → {plan.project.to} · {formatBytes(f.size_before)} → {formatBytes(f.size_after)}</span>
        {/if}
      </div>

      {#if f.message && f.status !== 'convert'}
        <div class="note">
          {f.message}{#if f.status === 'invalid' && f.detected && f.detected !== f.from} — it looks like {f.detected}{/if}
        </div>
      {/if}

      {#if f.unrepresentable_total > 0}
        <div class="note warn">
          {f.unrepresentable_total} character{f.unrepresentable_total === 1 ? '' : 's'} {plan.project.to} cannot hold
          would {fate}:
          {#each f.samples as u (u.line + ':' + u.column)}
            <span class="chr" use:tooltip={`${u.code} · line ${u.line}, column ${u.column}`}>{u.ch}</span>
          {/each}
          {#if f.unrepresentable_total > f.samples.length}…{/if}
        </div>
      {/if}

      {#if f.declaration}
        <div class="note">
          Declares <code>{f.declaration}</code> about itself{f.declaration_rewritten ? ' — will be updated.' : ' — left as is.'}
        </div>
      {/if}

      {#if bomNote(f)}
        <div class="note">{bomNote(f)}</div>
      {/if}
    </li>
  {/each}
</ul>

{#if plan.truncated}
  <div class="more">Showing the first {plan.files.length} files; the rest are converted the same way.</div>
{/if}

<style>
  .plan {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    padding: 6px 12px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    color: var(--text-primary);
  }
  .meta {
    flex-shrink: 0;
    font-size: var(--font-size-2xs);
    color: var(--text-tertiary);
  }
  .note {
    margin: 3px 0 0 0;
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
  }
  .note.warn {
    color: var(--warning);
  }
  .note code {
    font-family: var(--font-code);
  }
  .chr {
    display: inline-block;
    min-width: 1.3em;
    margin: 0 2px;
    padding: 0 3px;
    text-align: center;
    font-family: var(--font-code);
    color: var(--text-primary);
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
  }
  .more {
    padding: 8px 12px;
    font-size: var(--font-size-xs);
    color: var(--text-tertiary);
  }
</style>
