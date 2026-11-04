<script lang="ts">
  /**
   * RequestEditor — the upper half of the request tab: bar, toolbar, and the Params / Headers / Auth / Body /
   * Tests / Docs sub-tabs. Edits go straight into the tab's draft; the tab's dirty dot is a comparison with
   * what was last saved, so there is no "modified" flag to keep right.
   */
  import { metaGet } from '$lib/ipc/hirundo';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import { hirundoStore, type RequestTab } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoUiStore, type EditorTab } from '$lib/stores/hirundo/ui.svelte';
  import { authLabel } from '../request-model';
  import AuthEditor from './AuthEditor.svelte';
  import BodyEditor from './BodyEditor.svelte';
  import KeyValueEditor from './KeyValueEditor.svelte';
  import RequestBar from './RequestBar.svelte';
  import RequestToolbar from './RequestToolbar.svelte';
  import TestsEditor from './TestsEditor.svelte';

  let { tab }: { tab: RequestTab } = $props();

  const r = $derived(tab.request);
  const count = (rows: { enabled: boolean; key: string }[]) => rows.filter((x) => x.enabled && x.key).length;

  const items = $derived<TabItem[]>([
    { id: 'params', label: 'Params', badge: count(r.params) || undefined },
    { id: 'headers', label: 'Headers', badge: count(r.headers) || undefined },
    { id: 'auth', label: r.auth.type === 'inherit' ? 'Auth (inherits)' : `Auth (${authLabel(r.auth)})` },
    { id: 'body', label: 'Body', badge: r.body.kind === 'none' ? undefined : r.body.kind },
    { id: 'tests', label: 'Tests', badge: r.tests.trim() ? r.tests.split('\n').filter((l) => l.trim() && !l.trim().startsWith('--')).length : undefined },
    { id: 'docs', label: 'Docs' },
  ]);

  // What "inherit" resolves to: the nearest collection or folder above the request that sets auth.
  let inherited = $state('');
  $effect(() => {
    const project = hirundoStore.project;
    const path = tab.path;
    inherited = '';
    if (!project || !path) return;
    const dirs = path.split('/').slice(0, -1);
    let cancelled = false;
    void (async () => {
      for (let i = dirs.length; i > 0; i--) {
        try {
          const meta = await metaGet(project, dirs.slice(0, i).join('/'));
          if (meta.auth.type !== 'inherit') {
            if (!cancelled) inherited = `${authLabel(meta.auth)} from ${meta.name}`;
            return;
          }
        } catch { /* a folder without settings inherits */ }
      }
    })();
    return () => { cancelled = true; };
  });

  function edit(fn: () => void) {
    fn();
    hirundoStore.touch(tab);
  }
</script>

<div class="re">
  <RequestBar {tab} />
  <RequestToolbar {tab} />
  <div class="re-tabs">
    <Tabs {items} value={hirundoUiStore.editorTab} variant="underline" size="sm" ariaLabel="Request sections" onSelect={(id) => hirundoUiStore.setEditorTab(id as EditorTab)} />
  </div>

  <div class="re-body">
    {#if hirundoUiStore.editorTab === 'params'}
      <KeyValueEditor rows={r.params} bulk={hirundoUiStore.bulk} onchange={(rows) => edit(() => (tab.request.params = rows))} />
    {:else if hirundoUiStore.editorTab === 'headers'}
      <KeyValueEditor rows={r.headers} bulk={hirundoUiStore.bulk} keyPlaceholder="Header" onchange={(rows) => edit(() => (tab.request.headers = rows))} />
    {:else if hirundoUiStore.editorTab === 'auth'}
      <AuthEditor auth={r.auth} inheritedLabel={inherited} onchange={(a) => edit(() => (tab.request.auth = a))} />
    {:else if hirundoUiStore.editorTab === 'body'}
      <BodyEditor body={r.body} soap={r.soap} bulk={hirundoUiStore.bulk} onchange={(b) => edit(() => (tab.request.body = b))} onsoap={(x) => edit(() => (tab.request.soap = x))} />
    {:else if hirundoUiStore.editorTab === 'tests'}
      <TestsEditor {tab} />
    {:else}
      <div class="re-docs">
        <Textarea bind:value={tab.request.docs} minRows={10} maxRows={30} framed ariaLabel="Notes" placeholder="Notes about this request — Markdown." onkeydown={() => hirundoStore.touch(tab)} />
      </div>
    {/if}
  </div>
</div>

<style>
  .re { display: flex; flex-direction: column; min-height: 0; height: 100%; }
  .re-tabs { padding: 0 8px; border-bottom: 1px solid var(--border-subtle); }
  .re-body { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; }
  .re-docs { padding: 12px; }
</style>
