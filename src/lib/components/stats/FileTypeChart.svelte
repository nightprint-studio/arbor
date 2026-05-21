<script lang="ts">
  import type { FileChangeStat } from '$lib/types/git';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    fileTypeBreakdown,   // [ext, count][]  top 10
    mostChangedFiles,    // FileChangeStat[] top 20
  }: {
    fileTypeBreakdown: [string, number][];
    mostChangedFiles: FileChangeStat[];
  } = $props();

  const maxExt  = $derived(fileTypeBreakdown.length > 0 ? Math.max(...fileTypeBreakdown.map(([,c]) => c)) : 1);
  const maxFile = $derived(mostChangedFiles.length > 0 ? Math.max(...mostChangedFiles.map(f => f.change_count)) : 1);

  // Color palette for extension bars (cycles through hues)
  function extColor(i: number): string {
    const hues = [210, 150, 30, 280, 10, 180, 340, 60, 240, 100];
    return `hsl(${hues[i % hues.length]},55%,50%)`;
  }

  function basename(path: string): string {
    return path.split(/[/\\]/).pop() ?? path;
  }
</script>

<div class="file-charts">

  <!-- File-type breakdown: horizontal bars -->
  <section class="section">
    <h3 class="section-title">By File Type</h3>
    <div class="ext-bars">
      {#each fileTypeBreakdown as [ext, count], i}
        {@const pct = (count / maxExt) * 100}
        <div class="ext-row">
          <span class="ext-label">{ext}</span>
          <div class="bar-track">
            <div
              class="bar-fill"
              style="width:{pct}%; background:{extColor(i)};"
            ></div>
          </div>
          <span class="ext-count">{count.toLocaleString()}</span>
        </div>
      {/each}
      {#if fileTypeBreakdown.length === 0}
        <p class="empty">No data</p>
      {/if}
    </div>
  </section>

  <!-- Most-changed files: horizontal bars -->
  <section class="section">
    <h3 class="section-title">Most Changed Files <span class="section-note">(first 500 commits)</span></h3>
    <div class="file-bars">
      {#each mostChangedFiles as f}
        {@const pct = (f.change_count / maxFile) * 100}
        <div class="file-row" use:tooltip={f.path}>
          <span class="file-path">
            <span class="file-dir">{f.path.replace(/[^/\\]+$/, '')}</span><span class="file-name">{basename(f.path)}</span>
          </span>
          <div class="bar-track file-track">
            <div
              class="bar-fill file-bar"
              style="width:{pct}%;"
            ></div>
          </div>
          <span class="file-count">{f.change_count}</span>
        </div>
      {/each}
      {#if mostChangedFiles.length === 0}
        <p class="empty">No data</p>
      {/if}
    </div>
  </section>
</div>

<style>
  .file-charts { display: flex; flex-direction: column; gap: 28px; }

  .section { display: flex; flex-direction: column; gap: 10px; }

  .section-title {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    margin: 0;
  }
  .section-note {
    font-size: 10px;
    font-weight: 400;
    text-transform: none;
    letter-spacing: 0;
    color: var(--text-disabled);
  }

  /* Extension bars */
  .ext-bars { display: flex; flex-direction: column; gap: 6px; }

  .ext-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .ext-label {
    width: 80px;
    font-size: 11px;
    font-family: var(--font-code);
    color: var(--text-secondary);
    text-align: right;
    flex-shrink: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ext-count {
    width: 44px;
    font-size: 11px;
    font-family: var(--font-ui-sans);
    color: var(--text-muted);
    text-align: right;
    flex-shrink: 0;
  }

  /* File bars */
  .file-bars { display: flex; flex-direction: column; gap: 5px; }

  .file-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .file-path {
    width: 260px;
    font-size: 11px;
    font-family: var(--font-code);
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex-shrink: 0;
    direction: rtl;
    text-align: left;
  }
  .file-dir { color: var(--text-muted); }
  .file-name { color: var(--text-primary); }

  .file-count {
    width: 32px;
    font-size: 11px;
    font-family: var(--font-ui-sans);
    color: var(--text-muted);
    text-align: right;
    flex-shrink: 0;
  }

  /* Shared bar styles */
  .bar-track {
    flex: 1;
    height: 6px;
    background: var(--bg-hover);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .file-track { height: 4px; }

  .bar-fill {
    height: 100%;
    border-radius: var(--radius-sm);
    min-width: 2px;
    transition: width 0.4s cubic-bezier(0.16,1,0.3,1);
  }

  .file-bar { background: var(--accent); opacity: 0.65; }
  .file-bar:hover { opacity: 1; }

  .empty {
    font-size: 12px;
    color: var(--text-muted);
    font-style: italic;
    font-family: var(--font-ui-sans);
    margin: 0;
  }
</style>
