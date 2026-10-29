<script lang="ts">
  /**
   * "What will be created": the generator's own summary lines, then the files (for a project
   * written locally) or the command / download (for one that is not).
   */
  import { FileText, Terminal } from 'lucide-svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import type { ProjectPreview } from '$lib/ipc/bennu/new-project';

  let { preview }: { preview: ProjectPreview | null } = $props();
</script>

<FormSection label="What will be created">
  {#if !preview || (!preview.summary.length && !preview.files.length && !preview.command)}
    <p class="hint">Fill in the form to see what the project will contain.</p>
  {:else}
    <ul class="lines">
      {#each preview.summary as line (line)}<li>{line}</li>{/each}
    </ul>
    {#if preview.files.length}
      <div class="files" role="list" aria-label="Files">
        {#each preview.files as f (f)}
          <span class="file" role="listitem"><span class="ico"><FileText size={11} /></span>{f}</span>
        {/each}
      </div>
    {/if}
    {#if preview.command}
      <div class="cmd"><Terminal size={12} /><code>{preview.command}</code></div>
    {/if}
  {/if}
</FormSection>

<style>
  .hint { margin: 0; font-size: var(--font-size-xs); color: var(--text-disabled); }
  .lines { margin: 0 0 8px; padding-left: 16px; font-size: var(--font-size-sm); color: var(--text-secondary); line-height: 1.55; }
  .files {
    display: flex; flex-direction: column; gap: 1px;
    max-height: 132px; overflow-y: auto; padding: 6px 8px;
    background: var(--bg-base); border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
  }
  .file {
    display: flex; align-items: center; gap: 6px;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-secondary);
  }
  .ico { display: inline-flex; flex-shrink: 0; color: var(--text-disabled); }
  .cmd {
    display: flex; align-items: flex-start; gap: 7px; padding: 7px 9px;
    background: var(--bg-base); border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
    color: var(--text-muted);
  }
  .cmd code { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-secondary); word-break: break-all; }
</style>
