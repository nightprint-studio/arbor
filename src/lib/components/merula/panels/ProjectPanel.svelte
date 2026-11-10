<script lang="ts">
  /**
   * Project panel — merula's wiring of the shared `ProjectTree` (the same tool window Bennu's
   * Project sidebar is): a lazily-listed tree over the project folder, showing folders, `.merula`
   * files and the manifest. The tree owns the file-manager verbs; this says what is shown, how a
   * `.merula` row looks (note glyph, per-file summary, `lib` badge) and how merula's models follow a
   * rename / move / delete (open tabs, cached buffers).
   */
  import { FileMusic, BookLock } from 'lucide-svelte';
  import ProjectTree, { type ProjectTreeIcon } from '$lib/components/shared/internal/project-tree/ProjectTree.svelte';
  import type { ProjectNode } from '$lib/components/shared/internal/project-tree/project-tree-model';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { fsOpenDefault, type FsEntry } from '$lib/ipc/fs';
  import { projectStore } from '../stores/project.svelte';
  import { projectActions } from '../stores/project-actions.svelte';
  import { merulaGitStatusStore } from '../stores/git-status.svelte';
  import { fileMetaStore } from '../stores/file-meta.svelte';

  const isMerula = (name: string) => name.toLowerCase().endsWith('.merula');
  const root = $derived(projectStore.project?.path ?? null);

  /** Folders, `.merula` files and the manifest — nothing else is merula's business. */
  const entryFilter = (e: FsEntry) => e.is_dir || isMerula(e.name) || e.name === 'merula.toml';

  // Per-file summaries, parsed lazily once a file is listed (cached thereafter); the active file's
  // counts stay live as its buffer changes.
  $effect(() => {
    for (const f of projectStore.files) void fileMetaStore.ensure(f.path, projectStore.sourceOf(f.path) || undefined);
  });
  $effect(() => {
    const path = projectStore.activeFilePath;
    if (path) fileMetaStore.refresh(path, projectStore.activeSource);
  });

  // Git marks: asked again when the project, the file list or the dirty buffers change, and when
  // the window regains focus (a commit in another tool changes them with none of the others firing).
  $effect(() => {
    void projectStore.files;
    merulaGitStatusStore.refresh(root);
  });

  const libraryOf = (path: string) => projectStore.files.find((f) => f.path === path)?.library ?? false;

  function iconFor(node: ProjectNode): ProjectTreeIcon | undefined {
    if (node.is_dir || !isMerula(node.name)) return undefined;
    return libraryOf(node.path)
      ? { icon: BookLock, color: 'var(--text-muted)' }
      : { icon: FileMusic, color: 'var(--accent)' };
  }

  function open(path: string) {
    if (isMerula(path)) void projectStore.openFile(path);
    else void fsOpenDefault(path);
  }

  const afterMove = (from: string, to: string) => projectStore.adoptRenamed(from, to);
</script>

<svelte:window onfocus={() => merulaGitStatusStore.refresh(root)} />

<ProjectTree
  {root}
  title="Project"
  activePath={projectStore.activeFilePath}
  {entryFilter}
  reloadToken={projectStore.files}
  onOpen={open}
  newItems={[{ id: 'pattern', label: 'Pattern (.merula)', icon: FileMusic, run: () => projectActions.newFile() }]}
  {iconFor}
  gitBadge={(p, d) => merulaGitStatusStore.badgeFor(p, d)}
  renameTransform={(name, node) => (!node.is_dir && isMerula(node.name) && !isMerula(name) ? `${name}.merula` : name)}
  onRenamed={afterMove}
  onMoved={afterMove}
  onDeleted={(path) => projectStore.dropFile(path)}
  emptyMessage="No project open. Open a project (Ctrl+O) or create one (Ctrl+Shift+N)."
>
  {#snippet rowExtra(node)}
    {@const meta = !node.is_dir ? fileMetaStore.get(node.path) : undefined}
    {#if libraryOf(node.path)}
      <span use:tooltip={'Library — its tracks() output is ignored'}><Badge variant="tone" tone="neutral" size="sm" label="lib" /></span>
    {:else if meta}
      <span class="pp-meta" use:tooltip={meta.description ?? `${meta.tracks} track${meta.tracks === 1 ? '' : 's'}`}>{meta.tracks} tr</span>
    {/if}
  {/snippet}
</ProjectTree>

<style>
  .pp-meta { margin-left: auto; padding-left: 8px; flex: none; font-size: 10px; color: var(--text-faint); font-variant-numeric: tabular-nums; }
</style>
