<script lang="ts">
  /**
   * The left column of structural search: ready-made queries to start from, the ones run lately, and the
   * ones the user kept. Clicking one fills the query field; nothing runs until Ctrl+Enter.
   *
   * Reading a query is how the language is learned, so every entry shows the query itself under its title.
   */
  import { LayoutTemplate, History, Bookmark, Trash2, Save } from 'lucide-svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { bennuSsrStore as store, type KeptQuery } from '$lib/stores/bennu/ssr.svelte';
  import type { SsrDialect } from '$lib/ipc/bennu/ssr';
  import { SSR_EXAMPLES } from '../ssr-examples';

  type Section = 'templates' | 'recent' | 'saved';
  let section = $state<Section>('templates');
  let filter = $state('');
  let naming = $state('');

  const tabs = $derived<TabItem[]>([
    { id: 'templates', label: 'Templates', icon: LayoutTemplate },
    { id: 'recent', label: 'Recent', icon: History, badge: store.recent.length || undefined },
    { id: 'saved', label: 'Saved', icon: Bookmark, badge: store.saved.length || undefined },
  ]);

  interface Row { key: string; title: string; why?: string; query: string; replacement: string; dialect: SsrDialect; removable?: string }

  const rows = $derived.by<Row[]>(() => {
    const q = filter.trim().toLowerCase();
    const keep = (r: Row) => !q || `${r.title} ${r.why ?? ''} ${r.query}`.toLowerCase().includes(q);
    if (section === 'templates') {
      return SSR_EXAMPLES
        .filter((ex) => (ex.dialect ?? 'java') === store.dialect)
        .map((ex): Row => ({ key: ex.title, title: ex.title, why: ex.why, query: ex.query, replacement: ex.replacement ?? '', dialect: ex.dialect ?? 'java' }))
        .filter(keep);
    }
    // Every language is listed; picking one switches the modal to the language it was written in.
    const kept = (section === 'saved' ? store.saved : store.recent)
      .map((k: KeptQuery, i): Row => ({
        key: `${section}:${i}:${k.query}`,
        title: k.title || firstLine(k.query),
        query: k.query,
        replacement: k.replacement,
        dialect: k.dialect as SsrDialect,
        removable: section === 'saved' ? k.title : undefined,
      }));
    return kept.filter(keep);
  });
  const firstLine = (text: string) => text.split('\n')[0].trim() || '(empty)';

  function use(row: Row) {
    store.load(row.query, row.dialect, row.replacement);
  }

  function saveCurrent() {
    store.saveTemplate(naming);
    naming = '';
    section = 'saved';
  }

  const empty = $derived(
    section === 'recent' ? 'Queries you run show up here.'
      : section === 'saved' ? 'Write a query, name it below and keep it.'
        : 'No template for this language.',
  );
</script>

<div class="side">
  <div class="side-tabs">
    <Tabs items={tabs} value={section} variant="segmented" size="xs" ariaLabel="Templates, recent and saved queries"
          onSelect={(id) => (section = id as Section)} />
  </div>
  <div class="side-filter">
    <Input value={filter} placeholder="Filter…" size="sm" ariaLabel="Filter templates" oninput={(v) => (filter = v)} />
  </div>

  <div class="side-list" role="listbox" aria-label="Queries">
    {#each rows as row (row.key)}
      <div class="row">
        <button class="row-main" type="button" role="option" aria-selected="false" onclick={() => use(row)}>
          <span class="row-title">{row.title}</span>
          {#if row.why}<span class="row-why">{row.why}</span>{/if}
          <code class="row-query">{row.query}</code>
        </button>
        {#if row.removable}
          <IconButton tooltip="Delete this template" onclick={() => store.removeTemplate(row.removable ?? '')}>
            <Trash2 size={12} />
          </IconButton>
        {/if}
      </div>
    {:else}
      <EmptyState message={empty} />
    {/each}
    {#if section === 'recent' && store.recent.length}
      <div class="side-clear"><Button variant="ghost" size="xs" onclick={() => store.clearRecent()}>Clear history</Button></div>
    {/if}
  </div>

  <div class="side-save">
    <Input value={naming} placeholder="Name this query…" size="sm" ariaLabel="Template name"
           oninput={(v) => (naming = v)}
           onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); saveCurrent(); } }} />
    <Button variant="secondary" size="xs" disabled={!naming.trim() || !store.query.trim()} onclick={saveCurrent}>
      {#snippet iconStart()}<Save size={12} />{/snippet}
      Save
    </Button>
  </div>
</div>

<style>
  .side { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--bg-elevated); }
  .side-tabs { padding: 8px 8px 4px; }
  .side-filter { padding: 4px 8px 6px; }
  .side-list { flex: 1; min-height: 0; overflow: auto; padding: 0 4px 6px; }
  .row { display: flex; align-items: flex-start; border-radius: var(--radius-sm); }
  .row:hover { background: var(--bg-hover); }
  .row-main {
    flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;
    padding: 6px 8px; text-align: left; background: none; border: none; cursor: pointer;
  }
  .row-title { font-size: var(--font-size-xs); color: var(--text-primary); font-weight: 600; }
  .row-why { font-size: var(--font-size-3xs); color: var(--text-muted); }
  .row-query {
    font-family: var(--font-code); font-size: var(--font-size-3xs); color: var(--text-secondary);
    white-space: pre; overflow: hidden; text-overflow: ellipsis; max-height: 3.4em;
  }
  .side-clear { display: flex; justify-content: center; padding: 6px; }
  .side-save {
    display: flex; gap: 6px; align-items: center; padding: 8px;
    border-top: 1px solid var(--border-subtle);
  }
</style>
