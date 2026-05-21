<script lang="ts">
  /**
   * StudioPanel — built-in sidebar listing every data file in the active
   * repo that we have (or will have) a first-class viewer for: `.ron`,
   * `.json`, `.toml`.
   *
   * Kept deliberately generic so adding a new kind (YAML, MJS, …) is a
   * one-liner in `StudioFileKind` + a click handler here. Cross-ref over
   * the project-wide index is planned but not in this first pass — the
   * file list is already wide enough to power it when we wire it up.
   */

  import {
    Search, X, RefreshCw, Boxes, FileJson, FileText, Eye, EyeOff,
    Network,
    Check, Link2, Link2Off,
    AlertTriangle, ChevronDown, ChevronRight,
    Plus, FolderPlus, FilePlus,
  } from 'lucide-svelte';
  import Icon from '@iconify/svelte';
  import ronIcon from '@iconify-icons/vscode-icons/file-type-ron';

  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import Tree from '$lib/components/shared/ui/Tree.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import BindSchemaModal from '$lib/components/shared/BindSchemaModal.svelte';
  import FilePickerModal from '$lib/components/shared/FilePickerModal.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { studioStore } from '$lib/stores/studio.svelte';
  import { ronStudioStore } from '$lib/stores/ron-studio.svelte';
  import { jsonStudioStore } from '$lib/stores/json-studio.svelte';
  import { tomlStudioStore } from '$lib/stores/toml-studio.svelte';
  import { yamlStudioStore } from '$lib/stores/yaml-studio.svelte';
  import { propertiesStudioStore } from '$lib/stores/properties-studio.svelte';
  import { notificationsStore } from '$lib/stores/notifications.svelte';
  import type { StudioFileEntry, StudioFileKind } from '$lib/ipc/studio';
  import { tooltip } from '$lib/actions/tooltip';

  // ── Tree shape ────────────────────────────────────────────────────────────────

  interface DirNode  { kind: 'dir';  name: string; path: string; children: TNode[]; fileCount: number; }
  interface FileNode { kind: 'file'; name: string; path: string; entry: StudioFileEntry; }
  type TNode = DirNode | FileNode;

  // ── Reactive state — driven from the store + active tab ──────────────────────

  const activeTabId = $derived(tabsStore.activeTabId);

  // Trigger the initial scan + re-scan on tab switch. The store is smart
  // enough to skip the IPC when the cached `loadedTabId` already matches.
  $effect(() => {
    const tabId = activeTabId;
    if (!tabId) return;
    void studioStore.ensureLoadedFor(tabId);
    // Broken-ref scan lives here too — the sidebar shows the
    // project-wide list, so we want the data populated as soon as
    // the user lands on a repo. Dedupes server-side via
    // `brokenRefsTabId`; nukes happen on index-done + config edits.
    void studioStore.loadBrokenRefs(tabId);
  });

  /** Section expansion (sidebar broken-refs panel). Collapsed by
   *  default; the user opts in by clicking the section header. */
  let brokenSectionOpen = $state(false);

  /** Per-value expansion within the broken-refs list. Keyed by the
   *  orphan value so the same group stays expanded across tab
   *  switches. */
  let brokenExpanded = $state<Set<string>>(new Set());
  function toggleBrokenValue(v: string) {
    const next = new Set(brokenExpanded);
    if (next.has(v)) next.delete(v); else next.add(v);
    brokenExpanded = next;
  }

  /** Group the project-wide broken-refs list by orphan `value`, so
   *  the same missing target collapses into a single row that
   *  expands to its offending sites. */
  const brokenGroups = $derived.by(() => {
    const m = new Map<string, typeof studioStore.brokenRefs>();
    for (const r of studioStore.brokenRefs) {
      const arr = m.get(r.value);
      if (arr) arr.push(r);
      else m.set(r.value, [r]);
    }
    return [...m.entries()];
  });

  function openBrokenSite(r: typeof studioStore.brokenRefs[number]): void {
    // Use the universal openDoc entry — works whether the modal is
    // already open (just adds a tab) or closed (boots the modal).
    // Passing tab + synthetic relative_path lets the host resolve
    // schema bindings for external files (walk-up from absolute
    // path can't find the repo's `.ron-studio.toml`).
    void ronStudioStore.openDoc({
      path:         r.absolute_path,
      tabId:        activeTabId ?? undefined,
      relativePath: r.relative_path,
    });
  }

  /** "+" launcher state — picker mode (file vs folder) drives the
   *  FilePickerModal's `mode` prop. `null` ⇒ launcher closed. */
  let externalPicker = $state<{ mode: 'file' | 'folder' } | null>(null);

  function openExternalPicker(mode: 'file' | 'folder'): void {
    externalPicker = { mode };
  }

  async function onExternalPicked(picked: string): Promise<void> {
    externalPicker = null;
    if (!activeTabId) return;
    try {
      await studioStore.addExternal(activeTabId, picked);
      notificationsStore.add('External', 'Added to this project.', 'success');
    } catch (e) {
      notificationsStore.add('External', `Add failed: ${e}`, 'error');
    }
  }

  async function removeExternal(path: string): Promise<void> {
    if (!activeTabId) return;
    try {
      await studioStore.removeExternal(activeTabId, path);
      notificationsStore.add('External', 'Removed from this project.', 'success');
    } catch (e) {
      notificationsStore.add('External', `Remove failed: ${e}`, 'error');
    }
  }

  /** Items for the "+ External" dropdown launcher. */
  const externalLauncherItems = $derived.by<DropdownItem[]>(() => [
    {
      kind: 'item', id: 'folder', label: 'Folder…', icon: FolderPlus,
      subtitle: 'All .ron / .json / .toml files inside',
      onclick: () => openExternalPicker('folder'),
    },
    {
      kind: 'item', id: 'file', label: 'Single file…', icon: FilePlus,
      subtitle: 'A specific file outside the repo',
      onclick: () => openExternalPicker('file'),
    },
  ]);

  // One-time setup: install index event listeners + load the persistent
  // index settings. When the toggle is on, kick off the background
  // refresh job for the active tab so cross-ref / usages queries see
  // the cached data on the next query.
  $effect(() => {
    void studioStore.installIndexListeners();
    studioStore.ensureSettingsLoaded().then(() => {
      const tabId = activeTabId;
      if (tabId && studioStore.settings.use_index && !studioStore.indexJobRunning) {
        void studioStore.refreshIndex(tabId);
      }
    });
  });

  // ── Tree assembly — filter first, then fold the surviving paths into a
  //    nested dir/file structure. Doing it in this order means empty
  //    folders don't survive in the tree when the user types a filter. ──

  const filtered = $derived.by<StudioFileEntry[]>(() => {
    const f = studioStore.filter.trim().toLowerCase();
    const k = studioStore.activeKinds;
    const showHidden = studioStore.showExcluded;
    const all = studioStore.files;
    return all.filter(e => {
      if (!showHidden && e.excluded) return false;
      if (k.size > 0 && !k.has(e.kind)) return false;
      if (!f) return true;
      return e.relative_path.toLowerCase().includes(f)
          || e.name.toLowerCase().includes(f);
    });
  });

  const excludedCount = $derived(studioStore.files.filter(e => e.excluded).length);

  const tree = $derived.by<TNode[]>(() => {
    const root: DirNode = { kind: 'dir', name: '', path: '', children: [], fileCount: 0 };
    function ensureDir(parts: string[]): DirNode {
      let cur = root;
      for (let i = 0; i < parts.length; i++) {
        const segPath = parts.slice(0, i + 1).join('/');
        let next = cur.children.find(c => c.kind === 'dir' && c.path === segPath) as DirNode | undefined;
        if (!next) {
          next = { kind: 'dir', name: parts[i], path: segPath, children: [], fileCount: 0 };
          cur.children.push(next);
        }
        cur = next;
      }
      return cur;
    }
    for (const entry of filtered) {
      const segs = entry.relative_path.split('/');
      const parent = ensureDir(segs.slice(0, -1));
      parent.children.push({ kind: 'file', name: segs[segs.length - 1], path: entry.relative_path, entry });
    }
    // Sort each level: dirs first (alpha), then files (alpha). Count files
    // recursively so the count badge on each folder is meaningful.
    function sortAndCount(n: DirNode): number {
      let count = 0;
      n.children.sort((a, b) => {
        if (a.kind !== b.kind) return a.kind === 'dir' ? -1 : 1;
        return a.name.localeCompare(b.name);
      });
      for (const c of n.children) {
        if (c.kind === 'dir') count += sortAndCount(c);
        else count += 1;
      }
      n.fileCount = count;
      return count;
    }
    sortAndCount(root);
    return root.children;
  });

  // Auto-expand when filtering — the user types to narrow down, hiding
  // results behind a folder chevron is wrong UX in that mode.
  const autoExpanded = $derived.by<Set<string>>(() => {
    const f = studioStore.filter.trim();
    if (!f) return studioStore.expanded;
    const out = new Set<string>();
    function visit(n: TNode) {
      if (n.kind !== 'dir') return;
      out.add(n.path);
      for (const c of n.children) visit(c);
    }
    for (const n of tree) visit(n);
    return out;
  });

  // ── Per-kind counts for the chip row ─────────────────────────────────────────

  const kindCounts = $derived.by<Record<StudioFileKind, number>>(() => {
    const counts: Record<StudioFileKind, number> = { ron: 0, json: 0, toml: 0, yaml: 0, properties: 0 };
    for (const e of studioStore.files) counts[e.kind] += 1;
    return counts;
  });

  // ── Actions ──────────────────────────────────────────────────────────────────

  function refresh(): void {
    if (!activeTabId) return;
    void studioStore.refresh(activeTabId);
  }

  /** Manual cross-reference index rebuild — separate from the file
   *  list rescan above. Fires the same background job that the
   *  settings panel exposes; progress flows through the existing
   *  `indexProgress` chip in the panel header. No-op when already
   *  running so a double-click doesn't queue work behind itself. */
  function reindex(): void {
    if (!activeTabId || studioStore.indexJobRunning) return;
    void studioStore.refreshIndex(activeTabId);
  }

  function openEntry(e: StudioFileEntry): void {
    switch (e.kind) {
      case 'ron':
        // Pass tab + synthetic relative_path so the host can resolve
        // schema bindings for external files via cfg lookup — the
        // walk-up from the file's absolute path can't reach the
        // repo's `.ron-studio.toml` for files outside the tree.
        void ronStudioStore.openDoc({
          path:         e.absolute_path,
          tabId:        activeTabId ?? undefined,
          relativePath: e.relative_path,
        });
        return;
      case 'json':
        // Same rationale as RON: pass tab + relative_path so the host
        // can resolve sidecar bindings for external files via cfg lookup.
        void jsonStudioStore.openDoc({
          path:         e.absolute_path,
          tabId:        activeTabId ?? undefined,
          relativePath: e.relative_path,
        });
        return;
      case 'toml':
        void tomlStudioStore.openDoc({
          path:         e.absolute_path,
          tabId:        activeTabId ?? undefined,
          relativePath: e.relative_path,
        });
        return;
      case 'yaml':
        // Phase 5.a — read-only navigation. Same tabId + relative path
        // plumbing as RON/JSON/TOML so the future schema sidecar (5.c)
        // can resolve external bindings via cfg lookup.
        void yamlStudioStore.openDoc({
          path:         e.absolute_path,
          tabId:        activeTabId ?? undefined,
          relativePath: e.relative_path,
        });
        return;
      case 'properties':
        // Phase 6 — lossless line-based edit + cross-refs (every key
        // is a target, every value is a reference) + JSON Schema
        // sidecar.
        void propertiesStudioStore.openDoc({
          path:         e.absolute_path,
          tabId:        activeTabId ?? undefined,
          relativePath: e.relative_path,
        });
        return;
    }
  }

  function kindIcon(k: StudioFileKind) {
    return k === 'ron' ? ronIcon : null;
  }

  function kindLabel(k: StudioFileKind): string {
    switch (k) {
      case 'ron':        return 'RON';
      case 'json':       return 'JSON';
      case 'toml':       return 'TOML';
      case 'yaml':       return 'YAML';
      case 'properties': return '.properties';
    }
  }

  function fmtBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / 1024 / 1024).toFixed(1)} MB`;
  }

  // ── Right-click context menu state ────────────────────────────────────────────

  type CtxTarget =
    | { kind: 'file'; entry: StudioFileEntry }
    | {
        kind:     'dir';
        path:     string;
        name:     string;
        excluded: boolean;
        binding:  { rs_file: string; root_type: string } | undefined;
      };
  interface CtxState { x: number; y: number; target: CtxTarget; }
  let ctxMenu = $state<CtxState | null>(null);

  /** Target the BindSchemaModal points at — either a single file or a
   *  folder. The modal itself is agnostic: it just receives the
   *  pattern string to write into `[[overrides]].glob` and a label. */
  type BindTarget =
    | { kind: 'file'; entry: StudioFileEntry }
    | {
        kind:     'folder';
        path:     string;
        name:     string;
        glob:     string;
        initial:  { rs_file: string; root_type: string } | undefined;
      };
  let bindTarget = $state<BindTarget | null>(null);

  function openFileContext(e: MouseEvent, entry: StudioFileEntry) {
    e.preventDefault();
    e.stopPropagation();
    ctxMenu = { x: e.clientX, y: e.clientY, target: { kind: 'file', entry } };
  }
  function openDirContext(e: MouseEvent, dir: DirNode) {
    e.preventDefault();
    e.stopPropagation();
    ctxMenu = {
      x: e.clientX, y: e.clientY,
      target: {
        kind: 'dir',
        path: dir.path,
        name: dir.name,
        excluded: studioStore.isFolderExcluded(dir.path),
        binding:  studioStore.folderBinding(dir.path),
      },
    };
  }

  /** Lookup the registered-external entry that backs a given tree
   *  node's `relative_path`. Single-file externals show as direct
   *  `external/<name>` rows; folder externals are
   *  `external/<label>/…`. We match by checking the path tail OR
   *  by absolute-path equality. */
  function externalEntryFor(relPath: string, absPath?: string): { path: string; label?: string } | null {
    if (!relPath.startsWith('external/')) return null;
    const externals = studioStore.config.externals ?? [];
    if (absPath) {
      const hit = externals.find(e => e.path === absPath);
      if (hit) return hit;
    }
    // Folder root case: `external/<label>` → match by label
    const segs = relPath.split('/');
    if (segs.length >= 2) {
      const label = segs[1];
      const hit = externals.find(e => (e.label || '') === label
                                   || e.path.replace(/\\/g, '/').split('/').filter(Boolean).pop() === label);
      if (hit) return hit;
    }
    return null;
  }

  const ctxItems = $derived.by<MenuItem[]>(() => {
    const tgt = ctxMenu?.target;
    if (!tgt) return [];
    const items: MenuItem[] = [];
    if (tgt.kind === 'file') {
      const ent = tgt.entry;
      // Phase 4.c.b.2 follow-up: binding is now available for every
      // studio format, not just RON. The backend dispatches the schema
      // probe per format (RON → .rs, JSON → .schema.json, TOML → either).
      if (ent.schema) {
        items.push({
          id: 'unbind-schema',
          label: `Unbind schema (${ent.schema.root_type})`,
          icon: Link2Off,
          iconColor: 'var(--text-muted)',
        });
      } else {
        items.push({
          id: 'bind-schema',
          label: 'Bind schema…',
          icon: Link2,
          iconColor: 'var(--accent)',
        });
      }
      items.push({
        id: ent.excluded ? 'include' : 'exclude',
        label: ent.excluded ? 'Include in scans' : 'Exclude from scans',
        icon: ent.excluded ? Eye : EyeOff,
        iconColor: 'var(--text-muted)',
      });
      // External single-file: offer "Remove from project" so the
      // user can drop the registration without having to navigate
      // to a parent folder. The action targets the absolute path
      // stored in the config.
      if (ent.external) {
        const reg = externalEntryFor(ent.relative_path, ent.absolute_path);
        if (reg) {
          items.push({
            id:        'remove-external',
            label:     'Remove from project (external)',
            icon:      X,
            iconColor: 'var(--warning)',
          });
        }
      }
    } else {
      // Folder context: schema bind/unbind + include/exclude. The
      // binding here applies to every file under the folder (glob
      // `<path>/**`) — per-file overrides still win at read time.
      if (tgt.binding) {
        items.push({
          id: 'unbind-folder-schema',
          label: `Unbind folder schema (${tgt.binding.root_type})`,
          icon: Link2Off,
          iconColor: 'var(--text-muted)',
        });
      } else {
        items.push({
          id: 'bind-folder-schema',
          label: 'Bind schema to folder…',
          icon: Link2,
          iconColor: 'var(--accent)',
        });
      }
      items.push({
        id: tgt.excluded ? 'include-folder' : 'exclude-folder',
        label: tgt.excluded ? 'Include folder in scans' : 'Exclude folder from scans',
        icon: tgt.excluded ? Eye : EyeOff,
        iconColor: 'var(--text-muted)',
      });
      // Folder is an external registration root (e.g. the synthetic
      // `external/<label>` node) — offer to remove the whole
      // registration, which drops every descendant from the
      // project's view.
      const reg = externalEntryFor(tgt.path);
      if (reg) {
        items.push({
          id:        'remove-external-folder',
          label:     `Remove "${reg.label || reg.path}" from project`,
          icon:      X,
          iconColor: 'var(--warning)',
        });
      }
    }
    return items;
  });

  async function onCtxSelect(id: string) {
    const tgt = ctxMenu?.target;
    ctxMenu = null;
    if (!tgt || !activeTabId) return;
    if (tgt.kind === 'file') {
      const ent = tgt.entry;
      switch (id) {
        case 'bind-schema':
          bindTarget = { kind: 'file', entry: ent };
          return;
        case 'unbind-schema':
          try {
            await studioStore.unbindSchemaFor(activeTabId, ent.relative_path);
            notificationsStore.add('Schema', `${ent.name} unbound from its schema.`, 'success');
          } catch (e) {
            notificationsStore.add('Schema', `Unbind failed: ${e}`, 'error');
          }
          return;
        case 'exclude':
        case 'include':
          try {
            const now = await studioStore.toggleExcludeFor(activeTabId, ent.relative_path);
            notificationsStore.add(
              'Studio',
              now
                ? `${ent.name} excluded — hidden from scans.`
                : `${ent.name} included in scans again.`,
              'info',
            );
          } catch (e) {
            notificationsStore.add('Studio', `Toggle failed: ${e}`, 'error');
          }
          return;
        case 'remove-external': {
          const reg = externalEntryFor(ent.relative_path, ent.absolute_path);
          if (reg) await removeExternal(reg.path);
          return;
        }
      }
    } else {
      const glob = studioStore.folderExcludeGlob(tgt.path);
      switch (id) {
        case 'bind-folder-schema':
          bindTarget = {
            kind:    'folder',
            path:    tgt.path,
            name:    tgt.name,
            glob,
            initial: tgt.binding,
          };
          return;
        case 'unbind-folder-schema':
          try {
            await studioStore.unbindSchemaFor(activeTabId, glob);
            notificationsStore.add('Schema', `Folder ${tgt.name}/ unbound from its schema.`, 'success');
          } catch (e) {
            notificationsStore.add('Schema', `Unbind failed: ${e}`, 'error');
          }
          return;
        case 'exclude-folder':
        case 'include-folder':
          try {
            const now = await studioStore.toggleExcludeFor(activeTabId, glob);
            notificationsStore.add(
              'Studio',
              now
                ? `Folder ${tgt.name}/ excluded — every file inside is now skipped.`
                : `Folder ${tgt.name}/ included again.`,
              'info',
            );
          } catch (e) {
            notificationsStore.add('Studio', `Toggle failed: ${e}`, 'error');
          }
          return;
        case 'remove-external-folder': {
          const reg = externalEntryFor(tgt.path);
          if (reg) await removeExternal(reg.path);
          return;
        }
      }
    }
  }

  async function onBindSaved(rsFile: string, rootType: string, referenceFields: string[] | null) {
    const target = bindTarget;
    bindTarget = null;
    if (!target || !activeTabId) return;
    const [pattern, label] = target.kind === 'file'
      ? [target.entry.relative_path, target.entry.name]
      : [target.glob,                `${target.name}/`];
    try {
      await studioStore.bindSchemaFor(activeTabId, pattern, rsFile, rootType, referenceFields);
      notificationsStore.add('Schema', `${label} bound to ${rootType}.`, 'success');
    } catch (e) {
      notificationsStore.add('Schema', `Bind failed: ${e}`, 'error');
    }
  }
</script>

<PanelShell
  title="Studio"
  count={!studioStore.loading && studioStore.files.length > 0 ? studioStore.files.length : null}
>
  {#snippet icon()}<Boxes size={14} />{/snippet}

  {#snippet actions()}
    {#if studioStore.indexJobRunning && studioStore.indexProgress}
      {@const p = studioStore.indexProgress}
      {@const pct = p.total > 0 ? Math.round((p.processed / p.total) * 100) : 0}
      <span class="rs-index-progress"
            use:tooltip={`Indexing ${p.processed}/${p.total} files…`}>
        <RefreshCw size={10} class="spin" />
        <span>{pct}%</span>
      </span>
    {/if}
    {#if excludedCount > 0}
      <button class="ps-btn"
              class:ps-btn-active={studioStore.showExcluded}
              onclick={() => studioStore.toggleShowExcluded()}
              use:tooltip={studioStore.showExcluded ? 'Hide excluded files' : `Show ${excludedCount} excluded files`}>
        {#if studioStore.showExcluded}<Eye size={11} />{:else}<EyeOff size={11} />{/if}
      </button>
    {/if}
    <!-- Rebuild the cross-reference index. Distinct from the Rescan
         button below, which only re-walks the file list — this one
         actually re-parses every .ron and rebuilds the
         id/name → file-location lookup that powers Ctrl+click jumps
         and Find Usages. Useful after external edits (git pull,
         editor outside Arbor, …) when the cached index would
         otherwise lag behind disk. -->
    <button class="ps-btn"
            onclick={reindex}
            disabled={!activeTabId || studioStore.indexJobRunning}
            use:tooltip={studioStore.indexJobRunning
              ? 'Index rebuild in progress…'
              : 'Rebuild cross-reference index'}>
      <Network size={11} class={studioStore.indexJobRunning ? 'spin' : ''} />
    </button>
    <button class="ps-btn" onclick={refresh} disabled={studioStore.loading || !activeTabId} use:tooltip={'Rescan files'}>
      <RefreshCw size={11} class={studioStore.loading ? 'spin' : ''} />
    </button>
    <!-- "+ External" launcher — folder OR file outside the repo,
         registered in `.ron-studio.toml` and rendered alongside the
         project's own files. Useful for savegames in %APPDATA%,
         configs in ~/.config, content shipped on a separate
         volume — anything you want tracked-but-not-in-the-repo. -->
    <Dropdown
      items={externalLauncherItems}
      width="260px"
      position="fixed"
    >
      {#snippet trigger({ toggle, open })}
        <button class="ps-btn"
                class:ps-btn-active={open}
                onclick={toggle}
                disabled={!activeTabId}
                use:tooltip={'Add an external file or folder to this project'}
                aria-haspopup="menu"
                aria-expanded={open}
                aria-label="Add external">
          <Plus size={11} />
        </button>
      {/snippet}
    </Dropdown>
  {/snippet}

  {#snippet toolbar()}
    <!-- Kind chip row — toggle filters that survive across re-mounts -->
    <div class="kind-row">
      {#each (['ron', 'json', 'toml', 'yaml'] as StudioFileKind[]) as k}
        {@const active = studioStore.activeKinds.has(k)}
        <button
          class="kind-chip"
          class:kind-chip-active={active}
          class:kind-chip-empty={kindCounts[k] === 0}
          onclick={() => studioStore.toggleKind(k)}
          use:tooltip={`Show ${kindLabel(k)} only`}
        >
          {#if k === 'ron'}
            <Icon icon={ronIcon} width={12} height={12} />
          {:else if k === 'json'}
            <FileJson size={12} />
          {:else}
            <FileText size={12} />
          {/if}
          <span>{kindLabel(k)}</span>
          <span class="kind-count">{kindCounts[k]}</span>
        </button>
      {/each}
    </div>

    <!-- Filename / path filter -->
    <div class="search-row">
      <Search size={12} class="search-icon" />
      <input
        class="search-input"
        type="text"
        placeholder="Filter by name or path…"
        spellcheck="false"
        value={studioStore.filter}
        oninput={(e) => studioStore.setFilter((e.target as HTMLInputElement).value)}
      />
      {#if studioStore.filter}
        <button class="search-clear" onclick={() => studioStore.setFilter('')} use:tooltip={'Clear'}>
          <X size={11} />
        </button>
      {/if}
    </div>
  {/snippet}

  <div class="tree-body">
    <!-- Project-wide broken-references summary. Collapsed by default
         so it doesn't dominate the file tree; expands to show every
         orphan ref grouped by value (sorted alphabetically). The
         modal's Bindings panel shows the same data scoped to the
         active doc — this one is the cross-file overview. -->
    {#if studioStore.brokenRefs.length > 0}
      <div class="rs-broken-section">
        <button class="rs-broken-head"
                onclick={() => brokenSectionOpen = !brokenSectionOpen}
                aria-expanded={brokenSectionOpen}>
          <span class="rs-broken-caret" aria-hidden="true">
            {#if brokenSectionOpen}<ChevronDown size={11} />{:else}<ChevronRight size={11} />{/if}
          </span>
          <AlertTriangle size={12} class="rs-broken-icon" />
          <span class="rs-broken-title">Broken references</span>
          <span class="rs-broken-count">{studioStore.brokenRefs.length}</span>
          {#if studioStore.brokenRefsLoading}
            <RefreshCw size={10} class="spin rs-broken-spin" />
          {/if}
        </button>
        {#if brokenSectionOpen}
          <div class="rs-broken-body">
            {#each brokenGroups as [value, sites] (value)}
              {@const isOpen = brokenExpanded.has(value)}
              <div class="rs-broken-group">
                <button class="rs-broken-group-head"
                        class:expanded={isOpen}
                        onclick={() => toggleBrokenValue(value)}
                        aria-expanded={isOpen}>
                  <span class="rs-broken-caret" aria-hidden="true">
                    {#if isOpen}<ChevronDown size={10} />{:else}<ChevronRight size={10} />{/if}
                  </span>
                  <span class="rs-broken-value" use:tooltip={value}>{value}</span>
                  <span class="rs-broken-group-count">{sites.length}</span>
                </button>
                {#if isOpen}
                  <div class="rs-broken-sites">
                    {#each sites as s (s.absolute_path + '\x00' + s.field_path.join('/'))}
                      <button class="rs-broken-site"
                              onclick={() => openBrokenSite(s)}
                              use:tooltip={`${s.relative_path}\n${s.key_name}: ${s.value}`}>
                        <span class="rs-broken-site-field">{s.key_name}</span>
                        <span class="rs-broken-site-file">{s.file_name}</span>
                      </button>
                    {/each}
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    {#if !activeTabId}
      <div class="state-msg muted">Open a repository to see its data files.</div>

    {:else if studioStore.loading && studioStore.files.length === 0}
      <div class="state-msg muted">
        <RefreshCw size={14} class="spin" /> <span>Scanning repo…</span>
      </div>

    {:else if studioStore.error}
      <div class="state-msg err">{studioStore.error}</div>

    {:else if studioStore.files.length === 0}
      <div class="state-msg muted">No <code>.ron</code> / <code>.json</code> / <code>.toml</code> files in this repo.</div>

    {:else if filtered.length === 0}
      <div class="state-msg muted">No matches.</div>

    {:else}
      <Tree
        nodes={tree}
        getId={(n: TNode) => n.path}
        getChildren={(n: TNode) => n.kind === 'dir' ? n.children : undefined}
        expandedIds={autoExpanded}
        onExpandToggle={(id) => studioStore.toggleFolder(id)}
        selectable={(n: TNode) => n.kind === 'file'}
        indentSize={14}
        basePadding={4}
        ariaLabel="Studio file tree"
        onSelect={(n: TNode) => { if (n.kind === 'file') openEntry((n as FileNode).entry); }}
        onContextMenu={(n: TNode, e: MouseEvent) => {
          if (n.kind === 'file') openFileContext(e, (n as FileNode).entry);
          else                   openDirContext(e, n as DirNode);
        }}
        rowClass={(ctx: any) => {
          const n = ctx.node as TNode;
          if (n.kind === 'file') return n.entry.excluded ? 'rs-row-excluded' : '';
          return studioStore.isFolderExcluded(n.path) ? 'rs-row-excluded' : '';
        }}
        rowTitle={(n: TNode) => n.path || '(root)'}
      >
        {#snippet row({ node, expanded: isOpen }: { node: TNode; expanded: boolean })}
          {@const isExternalRoot = node.kind === 'dir'
            && (node.path === 'external' || externalEntryFor(node.path) !== null)}
          {#if node.kind === 'dir'}
            <span class="node-icon node-icon-folder">
              {#if isExternalRoot}
                <Link2 size={13} class="i-external" />
              {:else if isOpen}
                📂
              {:else}
                📁
              {/if}
            </span>
            <span class="node-name truncate" class:rs-name-external={isExternalRoot}>{node.name === 'external' ? 'External' : node.name}</span>
            {#if isExternalRoot && node.path !== 'external'}
              <span class="rs-badge rs-badge-external"
                    use:tooltip={'Registered external location — drops out of the project when removed via right-click.'}>
                <Link2 size={9} />
              </span>
            {/if}
            {#if studioStore.folderBinding(node.path)}
              {@const fb = studioStore.folderBinding(node.path)!}
              <span class="rs-badge rs-badge-bound"
                    use:tooltip={`Folder schema: ${fb.root_type}`}>
                <Check size={9} />
              </span>
            {/if}
            {#if studioStore.isFolderExcluded(node.path)}
              <span class="rs-badge rs-badge-excluded"
                    use:tooltip={'Folder excluded from Studio scans'}>
                <EyeOff size={9} />
              </span>
            {/if}
            <span class="node-count">{node.fileCount}</span>
          {:else}
            {@const ic = kindIcon(node.entry.kind)}
            <span class="node-icon">
              {#if ic}
                <Icon icon={ic} width={14} height={14} />
              {:else if node.entry.kind === 'json'}
                <FileJson size={13} class="i-json" />
              {:else}
                <FileText size={13} class="i-toml" />
              {/if}
            </span>
            <span class="node-name truncate" class:rs-name-external={node.entry.external}>{node.name}</span>
            {#if node.entry.external}
              <span class="rs-badge rs-badge-external"
                    use:tooltip={node.entry.absolute_path}>
                <Link2 size={9} />
              </span>
            {/if}
            {#if node.entry.schema}
              <span class="rs-badge rs-badge-bound"
                    use:tooltip={`Schema: ${node.entry.schema.root_type} · ${node.entry.schema.origin}`}>
                <Check size={9} />
              </span>
            {:else}
              <span class="rs-badge rs-badge-unbound"
                    use:tooltip={'No schema bound — right-click to bind'}>○</span>
            {/if}
            {#if node.entry.excluded}
              <span class="rs-badge rs-badge-excluded"
                    use:tooltip={'Excluded from Studio scans'}>
                <EyeOff size={9} />
              </span>
            {/if}
            <span class="node-meta">{fmtBytes(node.entry.size_bytes)}</span>
          {/if}
        {/snippet}
      </Tree>
    {/if}
  </div>
</PanelShell>

{#if ctxMenu}
  <ContextMenu
    items={ctxItems}
    x={ctxMenu.x}
    y={ctxMenu.y}
    onSelect={onCtxSelect}
    onClose={() => ctxMenu = null}
  />
{/if}

{#if bindTarget && activeTabId}
  {@const initialOverride = bindTarget.kind === 'file'
    ? (studioStore.fileOverride(bindTarget.entry.relative_path) ?? bindTarget.entry.schema ?? null)
    : (bindTarget.initial ?? null)}
  {@const bindFormatId = bindTarget.kind === 'file' ? bindTarget.entry.kind : 'toml'}
  <BindSchemaModal
    formatId={bindFormatId}
    relativePath={bindTarget.kind === 'file' ? bindTarget.entry.relative_path : bindTarget.glob}
    fileName={bindTarget.kind === 'file' ? bindTarget.entry.name : `${bindTarget.name}/`}
    targetKind={bindTarget.kind}
    initial={initialOverride}
    onSave={onBindSaved}
    onClose={() => bindTarget = null}
  />
{/if}

{#if externalPicker}
  <FilePickerModal
    mode={externalPicker.mode}
    title={externalPicker.mode === 'folder'
      ? 'Pick external folder'
      : 'Pick external file'}
    extensions={externalPicker.mode === 'file' ? ['ron', 'json', 'toml', 'yaml', 'yml'] : undefined}
    onConfirm={onExternalPicked}
    onCancel={() => externalPicker = null}
  />
{/if}

<style>
  /* ── Toolbar: kind chips + filter ─────────────────────────────────────── */
  .kind-row {
    display: flex;
    gap: 4px;
    padding: 4px 8px 0;
    flex-wrap: wrap;
  }
  .kind-chip {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 2px 7px 2px 5px;
    background: var(--bg-overlay);
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: 10px;
    font-size: 10.5px;
    font-weight: 500;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .kind-chip:hover:not(.kind-chip-empty) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .kind-chip-active {
    background: var(--accent-subtle);
    color: var(--accent);
    border-color: var(--accent);
  }
  .kind-chip-empty {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .kind-count {
    font-family: var(--font-code);
    font-size: 9.5px;
    color: var(--text-muted);
    padding: 0 4px;
    background: rgba(0,0,0,0.18);
    border-radius: 8px;
  }
  .kind-chip-active .kind-count {
    background: rgba(0,0,0,0.25);
    color: var(--accent);
  }

  .search-row {
    display: flex; align-items: center; gap: 4px;
    margin: 6px 8px 8px;
    padding: 0 6px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    height: 24px;
  }
  .search-row :global(.search-icon) { color: var(--text-muted); flex-shrink: 0; }
  .search-input {
    flex: 1;
    background: transparent;
    border: none;
    color: var(--text-primary);
    font-size: 11.5px;
    outline: none;
    min-width: 0;
    padding: 0;
  }
  .search-input::placeholder { color: var(--text-disabled); }
  .search-clear {
    display: flex; align-items: center; justify-content: center;
    width: 16px; height: 16px;
    background: transparent;
    color: var(--text-muted);
    border: none;
    border-radius: 3px;
    cursor: pointer;
    flex-shrink: 0;
  }
  .search-clear:hover { background: var(--bg-hover); color: var(--text-primary); }

  /* ── Body / tree rows ─────────────────────────────────────────────────── */
  .tree-body {
    flex: 1;
    overflow: auto;
    padding: 4px 0 8px;
  }
  .state-msg {
    display: flex; align-items: center; justify-content: center;
    gap: 6px;
    padding: 24px 12px;
    font-size: 11.5px;
    color: var(--text-secondary);
    text-align: center;
  }
  .state-msg.muted { color: var(--text-muted); }
  .state-msg.err   { color: var(--error, #e06c75); }
  .state-msg code  { font-family: var(--font-code); font-size: 10.5px; padding: 0 3px; background: var(--bg-overlay); border-radius: 3px; }

  /* ── Broken-references section ────────────────────────────────────
     Collapsible banner above the file tree. Warning palette so it
     reads as a signal rather than a navigation aid. Groups by
     orphan value; each group expands to show the offending sites
     (file + ref field key). */
  .rs-broken-section {
    margin: 0 4px 8px;
    border: 1px solid color-mix(in srgb, var(--warning, #e5c07b) 26%, var(--border-subtle));
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--warning, #e5c07b) 6%, transparent);
    overflow: hidden;
  }
  .rs-broken-head {
    display: flex; align-items: center; gap: 5px;
    width: 100%;
    padding: 5px 8px;
    background: transparent;
    border: none;
    color: var(--text-primary);
    font-size: 11px;
    cursor: pointer;
    text-align: left;
    transition: background var(--transition-fast);
  }
  .rs-broken-head:hover { background: color-mix(in srgb, var(--warning, #e5c07b) 10%, transparent); }
  .rs-broken-caret { display: flex; align-items: center; color: var(--text-muted); flex-shrink: 0; }
  :global(.rs-broken-icon) { color: var(--warning, #e5c07b); flex-shrink: 0; }
  .rs-broken-title { font-weight: 600; }
  .rs-broken-count {
    display: inline-flex; align-items: center;
    font-family: var(--font-code);
    font-size: 10px;
    font-weight: 700;
    padding: 0 6px;
    height: 14px;
    border-radius: 8px;
    color: var(--warning, #e5c07b);
    background: color-mix(in srgb, var(--warning, #e5c07b) 22%, transparent);
    margin-left: auto;
  }
  :global(.rs-broken-spin) { color: var(--warning, #e5c07b); }

  .rs-broken-body {
    padding: 2px 4px 4px;
    border-top: 1px solid color-mix(in srgb, var(--warning, #e5c07b) 18%, transparent);
    max-height: 240px;
    overflow-y: auto;
  }
  .rs-broken-group { display: flex; flex-direction: column; }
  .rs-broken-group-head {
    display: flex; align-items: center; gap: 5px;
    width: 100%;
    padding: 3px 6px;
    background: transparent;
    border: none;
    color: var(--text-primary);
    text-align: left;
    cursor: pointer;
    border-radius: 3px;
    transition: background var(--transition-fast);
  }
  .rs-broken-group-head:hover { background: var(--bg-hover); }
  .rs-broken-value {
    font-family: var(--font-code);
    font-size: 10.5px;
    color: var(--warning, #e5c07b);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }
  .rs-broken-group-count {
    font-family: var(--font-code);
    font-size: 9.5px;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .rs-broken-sites {
    display: flex; flex-direction: column;
    gap: 1px;
    padding: 1px 0 3px 18px;     /* indent under the parent caret */
  }
  .rs-broken-site {
    display: flex; align-items: baseline; gap: 6px;
    width: 100%;
    padding: 2px 6px;
    background: transparent;
    border: none;
    color: var(--text-secondary);
    text-align: left;
    cursor: pointer;
    border-radius: 3px;
    font-size: 10px;
    transition: background var(--transition-fast), color var(--transition-fast);
    min-width: 0;
  }
  .rs-broken-site:hover { background: var(--bg-hover); color: var(--text-primary); }
  .rs-broken-site-field {
    font-family: var(--font-code);
    font-size: 9.5px;
    color: var(--warning, #e5c07b);
    flex-shrink: 0;
  }
  .rs-broken-site-file {
    font-family: var(--font-code);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .node-icon {
    display: inline-flex; align-items: center; justify-content: center;
    width: 18px; height: 18px;
    flex-shrink: 0;
  }
  .node-icon-folder { font-size: 11px; }
  :global(.i-json) { color: #cbcb41; }
  :global(.i-toml) { color: #9c4221; }
  .node-name {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    color: var(--text-primary);
  }
  .node-name.truncate {
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .node-meta, .node-count {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-muted);
    flex-shrink: 0;
    margin-left: 4px;
  }
  .node-count {
    background: var(--bg-overlay);
    border-radius: 8px;
    padding: 0 6px;
  }

  /* ── Per-file badges (schema-binding + excluded) ──────────────────────── */
  .rs-badge {
    display: inline-flex; align-items: center; justify-content: center;
    width: 14px; height: 14px;
    border-radius: 50%;
    flex-shrink: 0;
    font-size: 9px;
    line-height: 1;
    margin-left: 4px;
  }
  .rs-badge-bound {
    background: color-mix(in srgb, var(--success, #98c379) 28%, transparent);
    color: var(--success, #98c379);
  }
  .rs-badge-unbound {
    background: var(--bg-overlay);
    color: var(--text-disabled);
    font-family: var(--font-code);
  }
  .rs-badge-excluded {
    background: color-mix(in srgb, var(--text-muted) 20%, transparent);
    color: var(--text-muted);
  }
  /* External-location badge — wears the accent so it visually
     pairs with the External group's tinted name (below). */
  .rs-badge-external {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
  }
  :global(.i-external) { color: var(--accent); }
  .rs-name-external { color: var(--accent); }

  /* Excluded rows dimmed + struck through so the user immediately sees
     which files are silently ignored by the scanners. */
  :global(.tree .tree-row.rs-row-excluded) { opacity: 0.55; }
  :global(.tree .tree-row.rs-row-excluded .node-name) {
    text-decoration: line-through;
    text-decoration-thickness: 1px;
  }

  /* Action button pressed state (Show-excluded toggle) */
  :global(.panel-shell .ps-actions .ps-btn.ps-btn-active) {
    background: var(--accent-subtle);
    color: var(--accent);
  }

  /* Live index-refresh progress chip — sits next to the action buttons
     when the background job is running. */
  .rs-index-progress {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 1px 6px;
    margin-right: 4px;
    font-size: 10px;
    font-family: var(--font-code);
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border-radius: 10px;
    line-height: 1;
  }

  /* Spin animation reused from app.css; keep a local copy in case the
     panel renders before the global rule registers. */
  :global(.spin) { animation: studio-spin 1s linear infinite; }
  @keyframes studio-spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }
</style>
