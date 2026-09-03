<script lang="ts">
  /**
   * Local History — what this file, this folder, or this project used to be.
   *
   * ## Why a dialog and not a tool window
   *
   * You open it, you find a moment, you close it. It is not something you keep on screen
   * while you work, the way Problems or the Terminal are, so it should not cost permanent
   * height in the editor. IntelliJ, which has both kinds of surface, puts this one in a
   * dialog for the same reason.
   *
   * ## One window, four scopes
   *
   * They are four phrasings of one question and they share the timeline, the diff and the
   * actions. **Deleted** is the one that earns its keep loudest: a file that no longer
   * exists has no row to right-click, so a list that does not depend on the filesystem is
   * the only route to its history — the part IntelliJ makes you reach through the old
   * folder's change sets, having remembered the folder yourself.
   *
   * ## The folder column is a merge, not a listing
   *
   * History knows the deleted files; the project tree knows the ones nobody ever edited.
   * Either alone is a folder listing that is quietly wrong in one direction, so the column
   * is both — with the ghosts drawn as ghosts.
   */
  import { History, RotateCcw, Tag, Copy, Check, Columns2, AlignJustify } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import TextDiff from '$lib/components/shared/ui/TextDiff.svelte';
  import HistoryTimeline from './history/HistoryTimeline.svelte';
  import HistoryDeletedList from './history/HistoryDeletedList.svelte';
  import HistoryFolderList, { type FolderRow } from './history/HistoryFolderList.svelte';
  import { rowsFromGroups, rowsFromRevisions, type TimelineRow } from './history/timeline-rows';
  import { bennuHistoryStore } from '$lib/stores/bennu/history.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { revisionContent } from '$lib/ipc/bennu/history';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { isKey } from '$lib/utils/keybindings';
  import type { TreeNode } from '$lib/types/bennu';

  const s = bennuHistoryStore;

  const scopeTabs = $derived<TabItem[]>([
    { id: 'file', label: 'This file', disabled: !s.file },
    { id: 'folder', label: 'Folder', disabled: !s.dir },
    { id: 'project', label: 'Project' },
    { id: 'deleted', label: 'Deleted', badge: s.deleted.length || undefined },
  ]);

  /** The timeline rows for whichever scope is showing. */
  const rows = $derived<TimelineRow[]>(
    s.scope === 'file' || s.scope === 'deleted'
      ? rowsFromRevisions(s.scope === 'file' ? s.file : (s.selected?.file ?? ''), s.revisions)
      : rowsFromGroups(s.timeline, (rel) => `${s.root.replace(/\/+$/, '')}/${rel}`),
  );

  const selectedRowId = $derived(
    s.scope === 'file' || s.scope === 'deleted'
      ? (s.selected?.revision ?? null)
      : (s.timeline.find((g) => g.files.some((f) => f.revision === s.selected?.revision))?.id ?? null),
  );

  /** The paths the currently selected operation touched — what the folder column marks. */
  const touched = $derived.by(() => {
    const group = s.timeline.find((g) => g.id === selectedRowId);
    return new Set((group?.files ?? []).map((f) => f.path));
  });

  /** The live tree node for a directory, so the folder column can show files that have
   *  never been edited. Absent for a directory outside the materialised tree. */
  function liveChildren(dirAbs: string): TreeNode[] {
    const want = dirAbs.replace(/\\/g, '/').replace(/\/+$/, '');
    const walk = (n: TreeNode): TreeNode[] | null => {
      if (n.path.replace(/\\/g, '/').replace(/\/+$/, '') === want) return n.children;
      for (const c of n.children) {
        if (!c.is_dir) continue;
        const hit = walk(c);
        if (hit) return hit;
      }
      return null;
    };
    const root = projectStore.tree;
    return root ? (walk(root) ?? []) : [];
  }

  /** History's entries merged with the live listing — the union, with the ghosts kept. */
  const folderRows = $derived.by<FolderRow[]>(() => {
    if (s.scope !== 'folder' && s.scope !== 'project') return [];
    const dirAbs = s.scope === 'project' ? s.root : s.dir;
    const byPath = new Map<string, FolderRow>();

    for (const n of liveChildren(dirAbs)) {
      const path = n.path.replace(/\\/g, '/');
      byPath.set(path, {
        name: n.name, path, isDir: n.is_dir, deleted: false, tracked: false, at: 0,
        inChange: touched.has(s.rel(path)),
      });
    }
    for (const e of s.entries) {
      // History speaks project-relative; every other call takes absolute. One join, and
      // the key is the same string the live listing produced, so the two merge instead of
      // doubling up.
      const abs = `${s.root.replace(/\\/g, '/').replace(/\/+$/, '')}/${e.path}`;
      byPath.set(abs, {
        name: e.name,
        path: abs,
        isDir: e.is_dir,
        deleted: e.deleted,
        tracked: true,
        at: e.at,
        inChange: touched.has(e.path),
      });
    }
    return [...byPath.values()].sort((a, b) =>
      Number(b.isDir) - Number(a.isDir) || a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
  });

  const selectedDeleted = $derived(
    s.scope === 'deleted' ? s.deleted.find((d) => `${s.root.replace(/\/+$/, '')}/${d.path}` === s.selected?.file) ?? null : null,
  );

  /** The filter box.
   *
   *  Local to the dialog rather than in the store: it is a property of *looking*, not of what
   *  is being looked at, and nothing outside this window needs it. Cleared when the scope
   *  changes, because a query typed against the deleted list means nothing against a folder. */
  let filterText = $state('');
  let lastScope = $state(s.scope);
  $effect(() => {
    if (s.scope !== lastScope) { lastScope = s.scope; filterText = ''; }
  });

  // ── actions ─────────────────────────────────────────────────────────────────
  let labelling = $state(false);
  let labelText = $state('');
  let copied = $state(false);
  let busy = $state(false);

  async function restore() {
    const sel = s.selected;
    if (!sel) return;
    busy = true;
    try {
      const at = await s.restore(sel.file, sel.revision);
      // Re-read it into the editor: leaving the buffer on the old text would mean the
      // file and the tab disagree, which is the exact confusion a restore is meant to end.
      await projectStore.reload(at);
      toastStore.show(`Restored ${s.rel(at)}`, 'success');
    } catch (e) {
      toastStore.show(e instanceof Error ? e.message : String(e), 'error');
    } finally {
      busy = false;
    }
  }

  async function restoreDeleted(path: string) {
    busy = true;
    try {
      const at = await s.restore(path);
      toastStore.show(`Restored ${s.rel(at)}`, 'success');
      projectStore.refreshTree();
    } catch (e) {
      toastStore.show(e instanceof Error ? e.message : String(e), 'error');
    } finally {
      busy = false;
    }
  }

  async function copyVersion() {
    const sel = s.selected;
    if (!sel) return;
    const { text } = await revisionContent(s.root, sel.file, sel.revision);
    await copyToClipboard(text);
    copied = true;
    setTimeout(() => (copied = false), 1400);
  }

  async function commitLabel() {
    const sel = s.selected;
    if (!sel) return;
    await s.label(sel.file, sel.revision, labelText.trim());
    labelling = false;
    labelText = '';
  }

  function onTimelineSelect(row: TimelineRow) {
    s.select({ file: row.file, revision: row.revision });
  }

  // ── how the diff is drawn ───────────────────────────────────────────────────
  //
  // Side by side by default: this window's question is "what did this line used to be", and a
  // unified patch answers it by putting the old form and the new one rows apart. Remembered in
  // the Bennu config rather than re-asked every time — a view mode is chosen once.
  const split = $derived(bennuSettingsStore.historyDiffSplit);

  /**
   * `Alt+D` flips the layout while the window is open.
   *
   * Not Corvus's `Alt+1` / `Alt+2` for the same two verbs, which would have been the consistent
   * choice: in a Bennu window those two already open the Project and Structure tool windows
   * (IntelliJ's own), and a key that means two things in one window means neither.
   *
   * Only while we are open, and never while a field has focus — the label box is one Tab away.
   * Matched with `isKey`, not against `event.key`: Option+D on a Mac arrives as `∂`.
   */
  function onWindowKeydown(e: KeyboardEvent) {
    if (!s.open || !e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
    if (!isKey(e, 'd')) return;
    const el = e.target as HTMLElement | null;
    if (el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable)) return;
    e.preventDefault();
    bennuSettingsStore.setHistoryDiffSplit(!split);
  }
</script>

<svelte:window onkeydown={onWindowKeydown} />

{#if s.open}
  <Modal
    onClose={() => s.close()}
    width="min(1580px, 96vw)"
    height="min(900px, 92vh)"
    padBody={false}
    ariaLabel="Local history"
  >
    {#snippet header()}
      <ModalHeader onClose={() => s.close()}>
        <History size={14} />
        <span class="modal-title">Local History</span>
        <span class="lh-subject">{s.subject}</span>
        <div class="lh-scopes">
          <Tabs
            items={scopeTabs}
            value={s.scope}
            variant="solid"
            size="sm"
            ariaLabel="History scope"
            onSelect={(id) => s.setScope(id as typeof s.scope)}
          />
        </div>
        <div class="lh-search">
          <SearchBar
            bind:query={filterText}
            placeholder="Filter…"
            showRegex={false}
            showCounter={false}
            ariaLabel="Filter history"
          />
        </div>
      </ModalHeader>
    {/snippet}

    <div class="lh">
      {#if s.error}
        <div class="lh-error"><Alert variant="error" text={s.error} compact /></div>
      {/if}

      <div class="lh-cols">
        <div class="lh-col lh-left">
          <div class="lh-col-h">
            {s.scope === 'deleted' ? 'Deleted files' : 'Revisions'}
            {#if s.loading}<Spinner size={11} />{/if}
            <span class="lh-count">{s.scope === 'deleted' ? s.deleted.length : rows.length}</span>
          </div>
          {#if s.scope === 'deleted'}
            <HistoryDeletedList
              entries={s.deleted}
              filter={filterText}
              selectedPath={selectedDeleted?.path ?? null}
              onSelect={(e) => void s.selectDeleted(e)}
            />
          {:else}
            <HistoryTimeline
              {rows}
              selectedId={selectedRowId}
              onSelect={onTimelineSelect}
              emptyMessage={s.scope === 'file'
                ? 'This file has no recorded revisions yet.'
                : 'Nothing has been recorded here yet.'}
            />
          {/if}
        </div>

        {#if s.scope === 'folder' || s.scope === 'project'}
          <div class="lh-col lh-mid">
            <div class="lh-col-h">
              In this folder
              {#if touched.size}<span class="lh-count">{touched.size} touched</span>{/if}
            </div>
            <HistoryFolderList
              rows={folderRows}
              filter={filterText}
              onOpen={(r) => s.show(s.root, r.path)}
              onRestore={(r) => void restoreDeleted(r.path)}
            />
          </div>
        {/if}

        <div class="lh-col lh-right">
          <div class="lh-col-h">
            {#if s.selected}
              <span class="lh-diff-of">{s.rel(s.selected.file)}</span>
              <span class="lh-vs">→ {s.scope === 'deleted' ? 'gone' : 'on disk'}</span>
            {:else}
              Diff
            {/if}
            <div class="lh-head-end">
              {#if s.delta && !s.delta.identical}
                <span class="lh-count">
                  <span class="add">+{s.delta.added}</span>
                  <span class="del">−{s.delta.removed}</span>
                </span>
              {/if}
              {#if s.diffing}<Spinner size={11} />{/if}
              <Button
                size="xs"
                variant="icon"
                ariaLabel={split ? 'Show a unified diff' : 'Show the two versions side by side'}
                tooltip={{
                  content: split ? 'Unified diff' : 'Side by side',
                  shortcut: 'Alt+D',
                }}
                onclick={() => bennuSettingsStore.setHistoryDiffSplit(!split)}
              >
                {#snippet iconStart()}
                  {#if split}<AlignJustify size={13} />{:else}<Columns2 size={13} />{/if}
                {/snippet}
              </Button>
            </div>
          </div>
          <div class="lh-diff">
            <TextDiff
              hunks={s.delta?.hunks ?? []}
              identical={s.delta?.identical ?? false}
              mode={split ? 'split' : 'unified'}
              emptyMessage="Pick a revision to see what it changed."
              identicalMessage="This revision is identical to what is on disk."
            />
          </div>
        </div>
      </div>
    </div>

    {#snippet footer()}
      <ModalFooter>
        {#if labelling}
          <div class="lh-label">
            <Input
              bind:value={labelText}
              placeholder="Name this moment…"
              size="sm"
              autofocus
              ariaLabel="Label for this revision"
              onkeydown={(e: KeyboardEvent) => {
                if (e.key === 'Enter') void commitLabel();
                if (e.key === 'Escape') { labelling = false; labelText = ''; }
              }}
            />
            <Button size="sm" variant="primary" onclick={() => void commitLabel()}>Label</Button>
            <Button size="sm" variant="ghost" onclick={() => { labelling = false; labelText = ''; }}>
              Cancel
            </Button>
          </div>
        {:else}
          <span class="lh-hint">
            Labelled revisions never expire. Everything else: kept for a while, then dropped.
          </span>
          <Button size="sm" variant="ghost" disabled={!s.selected} onclick={() => (labelling = true)}>
            {#snippet iconStart()}<Tag size={13} />{/snippet}
            Put label…
          </Button>
          <Button size="sm" variant="ghost" disabled={!s.selected} onclick={() => void copyVersion()}>
            {#snippet iconStart()}
              {#if copied}<Check size={13} />{:else}<Copy size={13} />{/if}
            {/snippet}
            {copied ? 'Copied' : 'Copy this version'}
          </Button>
          <Button size="sm" variant="primary" disabled={!s.selected || busy} onclick={() => void restore()}>
            {#snippet iconStart()}<RotateCcw size={13} />{/snippet}
            {s.scope === 'deleted' ? 'Restore where it was' : 'Restore this revision'}
          </Button>
        {/if}
      </ModalFooter>
    {/snippet}
  </Modal>
{/if}

<style>
  .lh { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .lh-error { padding: 8px 10px 0; }

  .lh-subject {
    font-family: var(--font-code); font-size: var(--font-size-xs);
    color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 260px;
  }
  .lh-scopes { margin-left: auto; }
  .lh-search { width: 180px; flex: none; }

  .lh-cols { display: flex; flex: 1; min-height: 0; }
  .lh-col { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .lh-col + .lh-col { border-left: 1px solid var(--border-subtle); }
  .lh-left { width: 320px; flex: none; }
  .lh-mid { width: 260px; flex: none; }
  .lh-right { flex: 1; }

  .lh-col-h {
    display: flex; align-items: center; gap: 8px; flex: none;
    height: 26px; padding: 0 10px;
    font-size: var(--font-size-2xs); letter-spacing: 0.05em; text-transform: uppercase;
    color: var(--text-faint); border-bottom: 1px solid var(--border-subtle);
  }
  .lh-count { margin-left: auto; display: flex; gap: 7px; text-transform: none; letter-spacing: 0; }
  /* The diff header's trailing group — counts, spinner, layout toggle — as ONE item, so a
     single auto margin puts the lot at the right edge whatever is currently in it. */
  .lh-head-end { margin-left: auto; display: flex; align-items: center; gap: 8px; }
  .lh-head-end .lh-count { margin-left: 0; }
  .lh-count .add { color: var(--success); }
  .lh-count .del { color: var(--error); }
  .lh-diff-of {
    font-family: var(--font-code); text-transform: none; letter-spacing: 0;
    color: var(--text-secondary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .lh-vs { text-transform: none; letter-spacing: 0; }

  .lh-diff { flex: 1; min-height: 0; }

  .lh-hint { margin-right: auto; font-size: var(--font-size-xs); color: var(--text-faint); }
  .lh-label { display: flex; align-items: center; gap: 8px; width: 100%; }
  .lh-label :global(.input-wrap) { flex: 1; }
</style>
