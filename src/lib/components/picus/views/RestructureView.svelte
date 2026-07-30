<script lang="ts">
  /**
   * Structural search and replace — the migration workspace.
   *
   * Reads top to bottom as the flow: what to look for → where → what came back →
   * what would change on disk. Nothing is written from here directly; the final
   * action names every file first, exactly like a generation, because it is the
   * same guarantee over the same scripts.
   *
   * ## Two features in one page, and the first works alone
   *
   * With no replacement this is a **query over the repository**, and the results
   * are a table with a column per placeholder. That half needs nothing else and is
   * exportable on its own. The replacement, when there is one, is checked against
   * every match before a preview is offered — a template that cannot be rendered
   * says so on the row it fails, not as one error for the whole migration.
   */
  import { Search, Replace, Play, Check, RefreshCw, GitCompare, FolderTree } from 'lucide-svelte';
  import Card from '$lib/components/shared/ui/Card.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import MatchTable from '../restructure/MatchTable.svelte';
  import ExportMatchesButton from '../restructure/ExportMatchesButton.svelte';
  import PatchDiffCard from '../generate/PatchDiffCard.svelte';
  import { sqlLanguage } from '../picus-sql-language';
  import CaptureGroups from '../restructure/CaptureGroups.svelte';
  import { restructureStore } from '$lib/stores/picus/restructure.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { engineLabel, type Dialect, type FolderRole } from '$lib/types/picus';

  const store = restructureStore;
  const showReplacement = $derived(!!store.replacement.trim());

  /**
   * The pattern is SQL, so it gets the SQL editor — highlighting **and completion
   * over the connected database's tables and columns**.
   *
   * Bound to the active connection rather than to nothing: a pattern is mostly
   * table and column names, and typing them from memory across a schema of four
   * hundred tables is where the typo that makes a pattern match nothing comes
   * from. The scope's engine decides the dialect where one is named, so a pattern
   * meant for Oracle is coloured as Oracle.
   */
  const conn = $derived(connectionsStore.active);
  const language = $derived(sqlLanguage(store.scope.engine ?? conn?.dialect ?? null, conn?.id));

  /** Folders offered as a scope, depth-first as the tree shows them. */
  const folderOptions = $derived([
    { value: '', label: 'Every folder' },
    ...picusProjectStore.entries
      .filter((e) => !e.node.effectiveExcluded)
      .map((e) => ({ value: e.node.path, label: e.node.path || '(the root)' })),
  ]);

  const ROLES: FolderRole[] = ['init', 'update', 'data', 'routines'];

  /**
   * Find, from inside either box.
   *
   * Claimed here rather than at the window: CodeMirror binds `Mod-Enter` to
   * "insert a blank line" and stops the event, so with the caret where it always
   * is the key would split the line and search nothing. The query editor pays for
   * the same lesson two files away.
   */
  const findKeys = [
    { key: 'Mod-Enter', preventDefault: true, run: () => { void store.find(); return true; } },
  ];

  function setScope(patch: Partial<typeof store.scope>) {
    const next = { ...store.scope, ...patch };
    // An empty string is "no filter", and the backend must not receive it as one
    // more predicate that matches nothing.
    for (const key of Object.keys(next) as (keyof typeof next)[]) {
      if (!next[key]) delete next[key];
    }
    store.setScope(next);
  }

  function open(match: { path: string; line: number }) {
    const file = picusProjectStore.fileByPath(match.path);
    picusTabsStore.openFile(match.path, file?.name ?? match.path, file?.effectiveEngine ?? null, match.line);
  }
</script>

<div class="rv">
  <header class="rv-head">
    <h1>Structural replace</h1>
    <p>
      The statement itself with holes in it: <code>$name$</code> is one node,
      <code>$name...$</code> a list. Leave the replacement empty and it is a query.
      <!-- The rest belongs in the docs, not above the box you came here to type in.
           A page whose explanation is taller than its controls is a page you scroll
           past every single time. -->
      <button type="button" class="rv-more" onclick={() => picusUiStore.openDocs('restructuring')}>
        How it works
      </button>
    </p>
  </header>

  {#if !picusProjectStore.attached}
    <Alert
      variant="info"
      title="No script repository is attached"
      text="Attach one to a connection and this searches it."
    />
  {:else}
    <div class="rv-cols">
      <Card padding="none">
        {#snippet header()}
          <span class="rv-card-title"><Search size={13} /> Pattern</span>
        {/snippet}
        <div class="rv-editor">
          <!-- Keyed on the descriptor: the editor builds its extensions at mount,
               so changing the connection — or the scope's engine — has to rebuild
               them, or the completion keeps offering the previous database's
               tables. Same reason as the query editor. -->
          {#key language}
            <CodeEditor
              value={store.pattern}
              {language}
              placeholder={'INSERT INTO CATALOGO_WIDGET ($cols...$) VALUES ($vals...$)'}
              wrap
              keyBindings={findKeys}
              oninput={(v) => store.setPattern(v)}
            />
          {/key}
        </div>
      </Card>

      <Card padding="none">
        {#snippet header()}
          <span class="rv-card-title"><Replace size={13} /> Replacement</span>
        {/snippet}
        {#snippet actions()}
          <span class="rv-hint">optional — leave empty to only search</span>
        {/snippet}
        <div class="rv-editor">
          {#key language}
            <CodeEditor
              value={store.replacement}
              {language}
              placeholder={'INSERT INTO CATALOGO_WIDGET (CHIAVE, ETICHETTA) VALUES ($vals[cols=chiave]$, $vals[cols=etichetta]$)'}
              wrap
              keyBindings={findKeys}
              oninput={(v) => store.setReplacement(v)}
            />
          {/key}
        </div>
      </Card>
    </div>

    <!-- ── Scope ────────────────────────────────────────────────────────────── -->
    <Card padding="none">
      {#snippet header()}
        <span class="rv-card-title"><FolderTree size={13} /> Where to look</span>
      {/snippet}
      {#snippet actions()}
        <Button
          variant="primary"
          size="sm"
          disabled={!store.pattern.trim() || store.searching}
          tooltip={{ content: 'Find every place this pattern matches', shortcut: 'Ctrl+Enter' }}
          onclick={() => void store.find()}
        >
          {#snippet iconStart()}<Play size={13} />{/snippet}
          Find
        </Button>
      {/snippet}

      <div class="rv-scope">
        <label class="rv-field">
          <span>Folder</span>
          <Select
            value={store.scope.folder ?? ''}
            options={folderOptions}
            searchable={folderOptions.length > 8}
            onchange={(v) => setScope({ folder: v || undefined })}
          />
        </label>
        <label class="rv-field">
          <span>Engine</span>
          <Select
            value={store.scope.engine ?? ''}
            options={[
              { value: '', label: 'Both' },
              { value: 'oracle', label: engineLabel('oracle') },
              { value: 'postgres', label: engineLabel('postgres') },
            ]}
            onchange={(v) => setScope({ engine: (v || undefined) as Dialect | undefined })}
          />
        </label>
        <label class="rv-field">
          <span>Role</span>
          <Select
            value={store.scope.role ?? ''}
            options={[{ value: '', label: 'Any' }, ...ROLES.map((r) => ({ value: r, label: r }))]}
            onchange={(v) => setScope({ role: (v || undefined) as FolderRole | undefined })}
          />
        </label>
      </div>
    </Card>

    <!-- ── What matched ─────────────────────────────────────────────────────── -->
    {#if store.searchError}
      <Alert variant="error" title="The pattern could not be used" text={store.searchError} />
    {:else if store.searched}
      <Card padding="none">
        {#snippet header()}
          <span class="rv-card-title">
            <Search size={13} /> Matches
            {#if store.searching}<Spinner size={11} />{/if}
          </span>
        {/snippet}
        {#snippet actions()}
          <Badge
            variant="tone"
            tone={store.matches.length ? 'accent' : 'neutral'}
            size="sm"
            label={`${store.matches.length} in ${store.fileCount} of ${store.scanned} scripts`}
          />
          {#if store.failing.length}
            <Badge
              variant="tone"
              tone="warning"
              size="sm"
              label={`${store.failing.length} cannot be rendered`}
            />
          {/if}
          {#if store.placeholders.length}
            <!-- The conflict view. Not a filter box: the question is "do these all
                 write it the same way", and a list of the distinct values answers
                 it in one glance where a filter would need you to already know
                 what to type. -->
            <label class="rv-inline">
              <span>Compare</span>
              <Select
                value={store.groupBy ?? ''}
                options={[
                  { value: '', label: 'nothing' },
                  ...store.placeholders.map((p) => ({ value: p, label: `$${p}$` })),
                ]}
                onchange={(v) => store.setGroupBy(v || null)}
              />
            </label>
          {/if}
          <ExportMatchesButton
            matches={store.visibleMatches}
            placeholders={store.placeholders}
            {showReplacement}
          />
          {#if store.matchesStale}
            <Button
              variant="primary"
              size="xs"
              tooltip="The pattern changed since these were found"
              onclick={() => void store.find()}
            >
              {#snippet iconStart()}<RefreshCw size={12} />{/snippet}
              Find again
            </Button>
          {/if}
        {/snippet}

        {#if store.matchesStale}
          <div class="rv-body">
            <Alert
              variant="warning"
              compact
              title="These are not the current matches"
              text="The pattern, the replacement or the scope changed since they were found."
            />
          </div>
        {/if}
        {#if store.failing.length}
          <div class="rv-body">
            <Alert
              variant="warning"
              compact
              title="The replacement cannot be rendered everywhere"
              text="Some matches caught a different number of elements than the template addresses. They are marked in the table; a rewrite is refused until every row can be written."
            />
          </div>
        {/if}

        {#if store.groupBy}
          <CaptureGroups
            groups={store.groups}
            selected={store.groupValue}
            name={store.groupBy}
            onSelect={(v) => store.showGroup(v)}
          />
        {/if}

        <!-- Bounded, and the grid scrolls inside it. The page scrolls too, and a
             list of ten thousand rows that only the PAGE could scroll is a page
             you can never reach the bottom of. -->
        <div class="rv-grid">
          <MatchTable
            matches={store.visibleMatches}
            placeholders={store.placeholders}
            {showReplacement}
            onOpen={open}
          />
        </div>
      </Card>
    {/if}

    <!-- ── What changes on disk ─────────────────────────────────────────────── -->
    {#if store.matches.length && showReplacement}
      <section class="rv-patches" aria-label="What the rewrite would change">
        <h2 class="rv-section">
          <GitCompare size={13} /> What would change on disk
          {#if store.previewing}<Spinner size={11} />{/if}
          <span class="rv-section-spacer"></span>
          <Button
            variant={store.stale || !store.preview ? 'primary' : 'ghost'}
            size="xs"
            disabled={!store.canPreview || store.previewing}
            tooltip={store.canPreview
              ? 'Compute the exact bytes this would write'
              : { content: 'Every match has to be renderable first' }}
            onclick={() => void store.buildPreview()}
          >
            {#snippet iconStart()}<RefreshCw size={12} />{/snippet}
            {store.preview ? 'Recompute' : 'Compute'}
          </Button>
          {#if store.preview}
            <Button
              variant="primary"
              size="xs"
              disabled={store.stale || store.writing}
              tooltip={store.stale
                ? { content: 'Recompute first — this describes an earlier pattern' }
                : 'Write the rewrite to the scripts'}
              onclick={() => void store.write()}
            >
              {#snippet iconStart()}<Check size={12} />{/snippet}
              Rewrite {store.preview.files.length} file{store.preview.files.length === 1 ? '' : 's'}
            </Button>
          {/if}
        </h2>

        {#if store.stale}
          <Alert
            variant="warning"
            compact
            title="This is not the current transformation"
            text="Something moved since it was computed. Recompute before writing; the write refuses a diff nobody reviewed."
          />
        {/if}

        {#if store.previewError}
          <Alert variant="error" title="The rewrite could not be computed" text={store.previewError} />
        {/if}

        {#if store.preview}
          {#each store.preview.refused as refusal (refusal.path)}
            <Alert variant="warning" compact title={refusal.path} text={refusal.reason} />
          {/each}
          {#each store.preview.files as file (file.path)}
            <!-- The generator's own diff card: same shape of answer, so it must
                 look and behave the same. `PreviewFile` is what it reads, and a
                 structural rewrite never creates a file — it can only change one
                 it matched in. -->
            <PatchDiffCard
              file={{
                path: file.path,
                before: file.before,
                after: file.after,
                encoding: file.encoding,
                eol: file.eol,
                reasons: [`${file.matches} match${file.matches === 1 ? '' : 'es'} rewritten`],
                createsFile: false,
                digest: file.digest,
              }}
            />
          {/each}
        {/if}
      </section>
    {/if}
  {/if}
</div>

<style>
  .rv {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    overflow: auto;
    flex: 1;
    min-height: 0;
    container-type: inline-size;
  }

  .rv-head h1 { font-size: var(--font-size-lg); font-weight: 600; color: var(--text-primary); }
  .rv-head p {
    margin-top: 3px;
    font-size: var(--font-size-xs);
    line-height: 1.5;
    color: var(--text-muted);
  }
  .rv-head code { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--accent); }

  .rv-more {
    background: none;
    border: none;
    padding: 0 0 0 4px;
    font-size: var(--font-size-xs);
    color: var(--accent);
    cursor: pointer;
    text-decoration: underline;
  }

  /* Pattern and replacement side by side while there is room: you move between
     them constantly. The measure is the PANEL's width, not the screen's. */
  .rv-cols { display: grid; grid-template-columns: 1fr; gap: 10px; }
  @container (min-width: 900px) {
    .rv-cols { grid-template-columns: 1fr 1fr; }
  }

  .rv-card-title {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--text-primary);
  }
  .rv-hint { font-size: var(--font-size-xs); color: var(--text-muted); }

  /* Tall enough for a statement that wraps, short enough that the matches stay in
     view — a pattern is two or three lines, not a document. */
  .rv-editor { height: 110px; display: flex; min-width: 0; }

  .rv-body { padding: 8px 10px; }

  /* The grid owns a bounded box and scrolls inside it. `55vh` rather than a fixed
     height so it uses a tall window without dwarfing a short one. */
  .rv-grid { height: 55vh; min-height: 240px; display: flex; flex-direction: column; }

  .rv-inline { display: inline-flex; align-items: center; gap: 5px; }
  .rv-inline span { font-size: var(--font-size-xs); color: var(--text-muted); }

  .rv-scope {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    padding: 10px;
  }
  .rv-field {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 180px;
    flex: 1;
  }
  .rv-field span { font-size: var(--font-size-xs); color: var(--text-secondary); }

  .rv-patches { display: flex; flex-direction: column; gap: 8px; }
  .rv-section {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--text-primary);
  }
  .rv-section-spacer { flex: 1; }
</style>
