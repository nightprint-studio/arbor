<script lang="ts">
  /**
   * Script file view — a file from the repository, opened in the editor.
   *
   * The banner above the buffer is the point: it states the encoding, the line
   * ending, and whether either has drifted from what the folder expects. Saving
   * preserves both; a character that cannot be represented in the destination
   * encoding blocks the save rather than being replaced by `?`.
   *
   * The buffer is highlighted with the branch's own dialect, because the same
   * text means different things in PL/SQL and PL/pgSQL.
   */
  import { TriangleAlert, FileCode2 } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import EncodingPill from '$lib/components/shared/internal/EncodingPill.svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import { sqlLanguage } from '../picus-sql-language';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import type { PicusTab } from '$lib/types/picus';

  interface Props {
    tab: PicusTab;
  }

  let { tab }: Props = $props();

  const file = $derived(tab.file ? picusProjectStore.fileByPath(tab.file) : null);
  const dialect = $derived(tab.file ? picusProjectStore.dialectOfFile(tab.file) : null);
  const language = $derived(sqlLanguage(dialect));
  const text = $derived(tab.file ? picusProjectStore.fileText(tab.file) : '');
  const drifted = $derived(!!file && file.encoding !== file.expectedEncoding);

  /** Which source decided this file's encoding — never leave the guess silent. */
  const SOURCE_LABEL = {
    bom: 'declared by a byte-order mark',
    utf8: 'valid UTF-8 with multibyte characters',
    inherited: 'pure ASCII — inherited from the folder',
    heuristic: 'single-byte heuristic',
    forced: 'pinned by hand',
  } as const;
</script>

{#if !file}
  <StateBlock tone="error" label="This file is no longer in the project index." />
{:else}
  <div class="fv">
    <div class="fv-bar" class:fv-bad={drifted}>
      <FileCode2 size={13} />
      <span class="fv-path">{file.path}</span>
      {#if dialect}<PicusDialectChip {dialect} />{/if}
      <EncodingPill
        encoding={file.encoding}
        expected={file.expectedEncoding}
        eol={file.eol}
        onChange={() => toastStore.show('Re-encoding arrives with the filesystem milestone.', 'info')}
      />
      <span class="fv-source">{SOURCE_LABEL[file.encodingSource]}</span>
      <span class="fv-spacer"></span>
      {#if drifted}
        <span class="fv-warn">
          <TriangleAlert size={12} />
          expected {file.expectedEncoding}
        </span>
        <Button
          variant="secondary"
          size="xs"
          onclick={() => toastStore.show('Conversion back to windows-1252 arrives with the filesystem milestone.', 'info')}
        >
          Convert back
        </Button>
      {/if}
    </div>

    <div class="fv-code">
      <CodeEditor
        value={text}
        {language}
        oninput={() => picusTabsStore.markDirty(tab.id, true)}
      />
    </div>
  </div>
{/if}

<style>
  .fv { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; }

  .fv-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    flex-shrink: 0;
    padding: 0 10px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
    font-size: 11.5px;
    white-space: nowrap;
  }
  /* A file whose encoding drifted is already wrong on disk — say so loudly. */
  .fv-bar.fv-bad {
    background: color-mix(in srgb, var(--error) 9%, var(--bg-elevated));
    border-bottom-color: color-mix(in srgb, var(--error) 30%, transparent);
  }
  .fv-bar :global(svg) { color: var(--text-disabled); flex-shrink: 0; }

  .fv-path {
    font-family: var(--font-code);
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 380px;
  }
  .fv-source { color: var(--text-disabled); font-size: 10.5px; }
  .fv-spacer { flex: 1; }
  .fv-warn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--error);
    font-weight: 600;
  }
  .fv-warn :global(svg) { color: var(--error); }

  .fv-code { flex: 1; min-height: 0; display: flex; overflow: hidden; }
  .fv-code > :global(*) { flex: 1; min-width: 0; min-height: 0; }
</style>
