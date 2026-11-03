<script lang="ts">
  /**
   * The jars Tomcat itself must load — JDBC drivers the project does not depend on, a logging
   * binding, anything that belongs in the server's `lib/` and not in the web application.
   *
   * A row is a jar or a folder; a folder stands for the `.jar` files directly in it, so a drivers
   * directory is one row and a jar dropped into it later is picked up on the next launch. The files
   * are copied into the generated base, never into the installation.
   */
  import { FolderOpen, Package, Plus, X } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import RowList from '$lib/components/shared/ui/RowList.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';

  let {
    paths,
    root,
    onchange,
  }: {
    paths: string[];
    root: string;
    onchange: (paths: string[]) => void;
  } = $props();

  let picking = $state<'jars' | 'folder' | null>(null);

  const name = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
  const isJar = (p: string) => p.toLowerCase().endsWith('.jar');

  /** Added to the end, once each — the same path twice would only be copied twice over itself. */
  function add(added: string[]) {
    const fresh = added.filter((p) => !paths.includes(p));
    if (fresh.length) onchange([...paths, ...fresh]);
    picking = null;
  }

  const remove = (path: string) => onchange(paths.filter((p) => p !== path));
</script>

<div class="libs">
  <div class="head">
    <span class="hint">
      Copied into the server's <code>lib/</code> at launch. A folder adds the <code>.jar</code> files in it.
    </span>
    <div class="actions">
      <Button variant="secondary" size="sm" onclick={() => (picking = 'jars')} tooltip={{ content: 'Choose one or more jar files' }}>
        {#snippet iconStart()}<Plus size={13} />{/snippet}
        Add jars…
      </Button>
      <Button variant="secondary" size="sm" onclick={() => (picking = 'folder')} tooltip={{ content: 'Choose a folder of jars' }}>
        {#snippet iconStart()}<FolderOpen size={13} />{/snippet}
        Add folder…
      </Button>
    </div>
  </div>

  {#if paths.length}
    <RowList items={paths} key={(p) => p}>
      {#snippet row(path)}
        <span class="ico">
          {#if isJar(path)}<Package size={13} />{:else}<FolderOpen size={13} />{/if}
        </span>
        <span class="text">
          <span class="name">{name(path)}</span>
          <span class="path" title={path}>{path}</span>
        </span>
        <IconButton tooltip="Remove" size={22} onclick={() => remove(path)}><X size={12} /></IconButton>
      {/snippet}
    </RowList>
  {:else}
    <p class="empty">None. The drivers the project depends on are copied on their own.</p>
  {/if}
</div>

{#if picking === 'jars'}
  <FileExplorerModal
    mode="file"
    multiple
    title="Choose jars for the server's lib/"
    extensions={['jar']}
    initialPath={root}
    onConfirm={(path) => add([path])}
    onConfirmMulti={add}
    onCancel={() => (picking = null)}
    onClose={() => (picking = null)}
  />
{:else if picking === 'folder'}
  <FileExplorerModal
    mode="folder"
    title="Choose a folder of jars"
    initialPath={root}
    onConfirm={(path) => add([path])}
    onCancel={() => (picking = null)}
    onClose={() => (picking = null)}
  />
{/if}

<style>
  .libs { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
  .head { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; }
  .hint { color: var(--text-muted); font-size: var(--font-size-xs); line-height: 1.5; }
  .actions { display: flex; gap: 6px; flex: none; }
  .ico { display: inline-flex; color: var(--text-muted); flex: none; }
  .text { display: flex; flex-direction: column; min-width: 0; flex: 1; }
  .name { font-size: var(--font-size-sm); color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .path {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left;
  }
  .empty { margin: 0; color: var(--text-muted); font-size: var(--font-size-xs); }
  code { font-family: var(--font-code); font-size: var(--font-size-xs); }
</style>
