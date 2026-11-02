<script lang="ts">
  /** The two ways in that are not "continue": open a project, clone one. */
  import { FolderOpen, Download } from 'lucide-svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';

  let { onopen, onclone }: { onopen: () => void; onclone: () => void } = $props();
</script>

<div class="actions">
  <button type="button" class="act primary" onclick={onopen}>
    <span class="ico"><FolderOpen size={16} /></span>
    <span class="txt"><b>Open project</b><small>A repository or a folder</small></span>
    <Kbd action="open_repo" size="sm" />
  </button>
  <button type="button" class="act" onclick={onclone}>
    <span class="ico"><Download size={16} /></span>
    <span class="txt"><b>Clone repository</b><small>From a URL or a provider</small></span>
  </button>
</div>

<style>
  .actions { display: flex; flex-wrap: wrap; gap: 10px; margin-top: 12px; }
  .act {
    display: flex; align-items: center; gap: 11px; padding: 9px 14px 9px 10px; cursor: pointer; color: inherit; font: inherit; text-align: left;
    background: var(--bg-elevated); border: 1px solid var(--border-subtle, var(--border)); border-radius: 9px;
    transition: border-color var(--anim-dur-base), background var(--anim-dur-base), transform var(--anim-dur-base);
  }
  .act:hover { background: var(--bg-hover); border-color: var(--border); transform: translateY(-1px); }
  .act:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .ico { display: grid; place-items: center; width: 30px; height: 30px; border-radius: 8px; background: var(--bg-base); color: var(--text-secondary); }
  .primary .ico { background: var(--accent); color: #fff; }
  .txt { display: flex; flex-direction: column; }
  b { font-weight: 600; color: var(--text-primary); font-size: var(--font-size-md); }
  small { font-size: var(--font-size-xs); color: var(--text-muted); }
</style>
