<script lang="ts">
  /** Files panel — the open project's `.merula` files, grouped by folder. Click
   *  opens the file as an editor tab; the New button scaffolds a starter file.
   *  Driven by the real `projectStore` (path-keyed). */
  import { FileMusic, BookLock, FolderOpen, Plus, Folder, FilePen, Trash2, MoreHorizontal } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import ContextMenu, { type MenuItem, type MenuAction } from '$lib/components/shared/ContextMenu.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { merulaStore } from '../merula-store.svelte';
  import { projectStore } from '../stores/project.svelte';
  import { projectActions } from '../stores/project-actions.svelte';
  import { fileMetaStore, type MerulaFileMeta } from '../stores/file-meta.svelte';
  import { revealFile } from '$lib/utils/reveal';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import type { MerulaProjectFile } from '$lib/ipc/merula/merula';

  const files = $derived(projectStore.files);
  const rootFiles = $derived(files.filter(f => !f.rel.includes('/')));
  const folders = $derived.by(() => {
    const map = new Map<string, MerulaProjectFile[]>();
    for (const f of files) {
      const slash = f.rel.lastIndexOf('/');
      if (slash === -1) continue;
      const dir = f.rel.slice(0, slash);
      const arr = map.get(dir) ?? [];
      arr.push(f); map.set(dir, arr);
    }
    return [...map.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  });

  // Folder sections expanded by default.
  let open = $state<Record<string, boolean>>({});
  const isOpen = (dir: string) => open[dir] ?? true;

  // Lazily parse each file's summary once it appears (cached thereafter); keep
  // the active file's counts live as its editor buffer changes.
  $effect(() => {
    for (const f of files) void fileMetaStore.ensure(f.path, projectStore.sourceOf(f.path) || undefined);
  });
  $effect(() => {
    const path = projectStore.activeFilePath;
    if (path) fileMetaStore.refresh(path, projectStore.activeSource);
  });

  // ── Per-file context menu (right-click / hover ⋯) → rename / delete / reveal ──
  // The rename + delete modals are hosted once in MerulaShell (shared with the
  // command palette); here we only open them via the store.
  let menu = $state<{ x: number; y: number; file: MerulaProjectFile } | null>(null);

  const menuActions: MenuAction[] = [
    { id: 'rename', label: 'Rename', icon: FilePen },
    { id: 'delete', label: 'Delete', icon: Trash2, danger: true },
  ];
  const menuItems: MenuItem[] = [
    { id: 'reveal', label: 'Reveal in File Explorer', icon: FolderOpen },
  ];

  function openMenu(e: MouseEvent, file: MerulaProjectFile) {
    menu = { x: e.clientX, y: e.clientY, file };
  }
  function onMenuSelect(id: string) {
    const file = menu?.file;
    menu = null;
    if (!file) return;
    if (id === 'rename') merulaStore.openRenameFile(file.path);
    else if (id === 'delete') merulaStore.openDeleteFile(file.path);
    // Through `utils/reveal`, not the FS command underneath it: that is where the
    // OS-vs-built-in-explorer choice (Settings → File Explorer) is honoured, and this panel
    // was the last reveal in the app that went around it and always used the system shell.
    else if (id === 'reveal')
      void revealFile(file.path).catch((e) =>
        toastStore.show(`Could not reveal ${file.name}: ${e}`, 'error'),
      );
  }

  function fmtBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(n < 10 * 1024 ? 1 : 0)} KB`;
    return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  }
  function metaLine(m: MerulaFileMeta): string {
    const parts = [`${m.tracks} track${m.tracks === 1 ? '' : 's'}`];
    if (m.sections) parts.push(`${m.sections} section${m.sections === 1 ? '' : 's'}`);
    if (m.cps != null) parts.push(`${+m.cps.toFixed(3)} cps`);
    parts.push(fmtBytes(m.bytes));
    return parts.join(' · ');
  }
</script>

{#snippet fileRow(f: MerulaProjectFile)}
  {@const active = projectStore.activeFilePath === f.path}
  {@const meta = fileMetaStore.get(f.path)}
  <SidebarItem
    selected={active}
    onclick={() => projectStore.openFile(f.path)}
    oncontextmenu={(e) => openMenu(e, f)}
  >
    {#snippet icon()}
      {#if f.library}<BookLock size={13} />{:else}<FileMusic size={13} />{/if}
    {/snippet}
    {f.name}
    {#snippet subtitle()}
      {#if meta?.description}
        <span use:tooltip={meta.description}>{meta.description}</span>
      {:else}
        {meta ? metaLine(meta) : f.rel}
      {/if}
    {/snippet}
    {#snippet badges()}
      {#if active}<span class="active-dot" use:tooltip={'Open in the editor'}></span>{/if}
      {#if f.library}<span use:tooltip={'Library — its tracks() output is ignored'}><Badge variant="tone" tone="neutral" size="sm" label="lib" /></span>{/if}
    {/snippet}
    {#snippet actions()}
      <button onclick={(e) => { e.stopPropagation(); openMenu(e, f); }} use:tooltip={'File actions'} aria-label="File actions"><MoreHorizontal size={14} /></button>
    {/snippet}
  </SidebarItem>
{/snippet}

<PanelShell title="Files" count={files.length}>
  {#snippet icon()}<FolderOpen size={13} />{/snippet}
  {#snippet actions()}
    <button class="ps-btn ps-btn-accent" onclick={() => projectActions.newFile()} use:tooltip={{ content: 'New .merula', shortcut: 'Ctrl+N' }} aria-label="New file"><Plus size={14} /></button>
  {/snippet}

  {#if files.length === 0}
    <EmptyState message="No project open. Open a project (Ctrl+O) or create one (Ctrl+Shift+N)." />
  {:else}
    <div class="files">
      {#each rootFiles as f (f.path)}{@render fileRow(f)}{/each}

      {#each folders as [dir, dirFiles] (dir)}
        <SidebarSection
          label={dir + '/'}
          expanded={isOpen(dir)}
          onToggle={() => open = { ...open, [dir]: !isOpen(dir) }}
          badge={dirFiles.length}
        >
          {#snippet icon()}<Folder size={13} />{/snippet}
          {#each dirFiles as f (f.path)}{@render fileRow(f)}{/each}
        </SidebarSection>
      {/each}
    </div>
  {/if}
</PanelShell>

{#if menu}
  <ContextMenu
    x={menu.x} y={menu.y}
    actions={menuActions}
    items={menuItems}
    onSelect={onMenuSelect}
    onClose={() => (menu = null)}
  />
{/if}

<style>
  .files { padding: 4px 0; }
  /* "Open in editor" marker — a small accent dot trailing the active file. */
  .active-dot {
    width: 6px; height: 6px; border-radius: 50%;
    background: var(--accent); flex-shrink: 0;
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 22%, transparent);
  }
</style>
