<script lang="ts">
  /**
   * What a replacement would do, before anything is written: every file it touches with its changed hunks,
   * and a box to tick the ones to apply.
   */
  import { ChevronDown, ChevronRight, FileCode2 } from 'lucide-svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import Checkbox from '$lib/components/shared/ui/Checkbox.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuSsrStore as store } from '$lib/stores/bennu/ssr.svelte';
  import { changeCounts, hunksOf } from './ssr-diff';
  import { plural } from './ssr-model';

  const preview = $derived(store.preview);
  const files = $derived(preview?.files ?? []);
  const ticked = $derived(files.filter((f) => store.applyFiles.has(f.file)).length);
  /** Every file starts open; the user closes the ones he has read. */
  const folded = new SvelteSet<string>();

  const view = $derived(
    files.map((f) => ({ file: f, counts: changeCounts(f.before, f.after), hunks: hunksOf(f.before, f.after, 2) })),
  );
</script>

{#if preview}
  <div class="rp">
    <div class="rp-head">
      <Checkbox
        checked={ticked === files.length && files.length > 0}
        indeterminate={ticked > 0 && ticked < files.length}
        ariaLabel="Select every file"
        onchange={(v) => store.setAllApplyFiles(v)}
      />
      <span>
        <strong>{preview.hits}</strong> {preview.hits === 1 ? 'change' : 'changes'} in
        <strong>{plural(files.length, 'file')}</strong> — {ticked} selected
      </span>
    </div>
    <div class="rp-list">
      {#each view as { file, counts, hunks } (file.file)}
        <div class="rp-file">
          <div class="rp-file-row">
            <Checkbox
              checked={store.applyFiles.has(file.file)}
              ariaLabel={`Apply to ${file.rel}`}
              onchange={(v) => store.toggleApplyFile(file.file, v)}
            />
            <button class="rp-file-btn" type="button"
                    onclick={() => (folded.has(file.file) ? folded.delete(file.file) : folded.add(file.file))}>
              {#if folded.has(file.file)}<ChevronRight size={12} />{:else}<ChevronDown size={12} />{/if}
              <FileCode2 size={12} />
              <span class="rp-name" use:tooltip={file.rel}>{file.rel}</span>
              <span class="rp-n-add">+{counts.added}</span>
              <span class="rp-n-del">−{counts.removed}</span>
            </button>
          </div>
          {#if !folded.has(file.file)}
            <div class="rp-diff">
              {#each hunks as hunk, i (i)}
                {#if i > 0}<div class="rp-gap">⋯</div>{/if}
                {#each hunk.lines as line, k (k)}
                  <div class="rp-line rp-{line.kind}">
                    <span class="rp-no">{line.oldNo ?? ''}</span>
                    <span class="rp-no">{line.newNo ?? ''}</span>
                    <span class="rp-sign">{line.kind === 'add' ? '+' : line.kind === 'del' ? '−' : ' '}</span>
                    <span class="rp-text">{line.text}</span>
                  </div>
                {/each}
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .rp { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .rp-head {
    display: flex; align-items: center; gap: 8px; flex-shrink: 0;
    padding: 7px 10px; border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs); color: var(--text-secondary);
  }
  .rp-list { flex: 1; min-height: 0; overflow: auto; }
  .rp-file { border-bottom: 1px solid var(--border-subtle); }
  .rp-file-row { display: flex; align-items: center; gap: 6px; padding: 0 10px; background: var(--bg-elevated); }
  .rp-file-btn {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px; padding: 5px 0;
    background: none; border: none; cursor: pointer; text-align: left;
    color: var(--text-secondary); font-size: var(--font-size-xs);
  }
  .rp-name {
    flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-code); font-size: var(--font-size-2xs); direction: rtl; text-align: left;
  }
  .rp-n-add { color: var(--success); font-family: var(--font-code); font-size: var(--font-size-2xs); }
  .rp-n-del { color: var(--error); font-family: var(--font-code); font-size: var(--font-size-2xs); }
  .rp-diff { background: var(--bg-base); font-family: var(--font-code); font-size: var(--font-size-2xs); line-height: 1.55; }
  .rp-gap { padding: 1px 10px 1px 70px; color: var(--text-disabled); }
  .rp-line { display: flex; white-space: pre; }
  .rp-no { flex-shrink: 0; width: 34px; padding-right: 6px; text-align: right; color: var(--text-disabled); user-select: none; }
  .rp-sign { flex-shrink: 0; width: 14px; text-align: center; user-select: none; }
  .rp-text { flex: 1; min-width: 0; padding-right: 10px; }
  .rp-line.rp-add { background: color-mix(in srgb, var(--success) 14%, transparent); }
  .rp-line.rp-add .rp-sign { color: var(--success); }
  .rp-line.rp-del { background: color-mix(in srgb, var(--error) 14%, transparent); }
  .rp-line.rp-del .rp-sign { color: var(--error); }
</style>
