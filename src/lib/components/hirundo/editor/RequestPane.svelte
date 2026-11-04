<script lang="ts">
  /**
   * RequestPane — the centre of the window. No project: the way to open one. A project and no open request:
   * the way to start one. Otherwise the tab strip and the request editor; the answer is in the bottom panel
   * and the environment in the title bar.
   */
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoCommands as c } from '../hirundo-commands';
  import RequestEditor from './RequestEditor.svelte';
  import RequestTabs from './RequestTabs.svelte';

</script>

<div class="rp">
  {#if !hirundoStore.project}
    <div class="rp-start">
      <EmptyState message="No project yet" description="A project is a folder with a .hirundo folder inside it, next to the code: your collections, environments and tests, as readable files you can commit." />
      <div class="rp-cta">
        <Button variant="primary" tooltip={{ content: 'Choose a folder', shortcut: 'Ctrl+Shift+O' }} onclick={c.openProject}>Open folder…</Button>
        <Button variant="outline" tooltip={{ content: 'Choose where it lives and name it', shortcut: 'Ctrl+Shift+P' }} onclick={c.newProject}>New project…</Button>
      </div>
      {#if hirundoStore.recents.length}
        <div class="rp-recents">
          {#each hirundoStore.recents.slice(0, 6) as path (path)}
            <SidebarItem onclick={() => void hirundoStore.openProject(path)}>{path}</SidebarItem>
          {/each}
        </div>
      {/if}
    </div>
  {:else}
    <div class="rp-tabs"><RequestTabs /></div>
    {#if hirundoStore.active}
      {#key hirundoStore.active.id}
        <div class="rp-editor"><RequestEditor tab={hirundoStore.active} /></div>
      {/key}
    {:else}
      <div class="rp-start">
        <EmptyState message="No request open" description="Pick one in the tree, press Ctrl+P to find it, or start a new one." />
        <div class="rp-cta">
          <Button variant="primary" onclick={c.newRequest}>New request</Button>
          <Button variant="outline" onclick={c.curl}>Paste cURL…</Button>
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .rp { display: flex; flex-direction: column; flex: 1; min-width: 0; min-height: 0; }
  .rp-tabs { padding-top: 6px; }
  .rp-editor { flex: 1; min-height: 0; overflow: hidden; }
  .rp-start { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; padding: 24px; }
  .rp-cta { display: flex; gap: 8px; }
  .rp-recents { width: min(420px, 90%); }
</style>
