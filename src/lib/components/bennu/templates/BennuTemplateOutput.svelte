<script lang="ts">
  /**
   * What a template rendered, and where it would go — the part every "generate from a template" dialog
   * shows the same way: the destination, what is worth knowing, and the code itself, read-only.
   */
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { baseName } from '$lib/utils/paths';
  import type { RenderedTemplate } from '$lib/ipc/bennu/templates';
  import type { languageForPath } from '../languages';

  let {
    rendered,
    root,
    text,
    language,
  }: {
    rendered: RenderedTemplate;
    /** The project root, for paths shown relative to it. */
    root: string | null;
    /** What is shown — the members as inserted, or the whole text. */
    text: string;
    language: ReturnType<typeof languageForPath>;
  } = $props();

  function relative(path: string): string {
    return root && path.startsWith(root) ? path.slice(root.length).replace(/^[\\/]/, '') : path;
  }
</script>

<div class="dest">
  {#if rendered.output === 'members'}
    <span>Goes into the class in <strong>{rendered.file ? baseName(rendered.file) : ''}</strong></span>
  {:else if rendered.output === 'file'}
    <span use:tooltip={rendered.file ?? ''}>
      {rendered.exists ? 'Already exists:' : 'A new file:'} <strong>{rendered.file ? relative(rendered.file) : ''}</strong>
    </span>
  {:else}
    <span>Text to copy, or to append to a file of the project</span>
  {/if}
  {#each rendered.notes as note, i (i)}<span class="note">{note}</span>{/each}
</div>
<div class="code">
  {#key language}<CodeEditor value={text} {language} readOnly />{/key}
</div>

<style>
  .dest { display: flex; flex-direction: column; gap: 2px; padding: 4px 12px 8px; font-size: var(--font-size-sm); color: var(--text-secondary); }
  .dest strong { font-family: var(--font-code); font-weight: 500; color: var(--text-primary); }
  .note { font-size: var(--font-size-xs); color: var(--text-muted); }
  .code { flex: 1; min-height: 0; display: flex; flex-direction: column; border-top: 1px solid var(--border-subtle); }
</style>
