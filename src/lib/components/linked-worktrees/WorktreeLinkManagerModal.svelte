<script lang="ts">
  import { cubicOut } from 'svelte/easing';
  import {
    Layers, Plus, Trash2, Edit3, Check, X, AlertCircle, GitBranch, Folder,
    RefreshCw, Link2, ChevronDown, Power,
  } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import ModalSidebarToggle from '$lib/components/shared/ui/ModalSidebarToggle.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { uiStore }    from '$lib/stores/ui.svelte';
  import { linkedWorktreesStore } from '$lib/stores/linkedWorktrees.svelte';
  import { workspacesStore } from '$lib/stores/workspaces.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { keybindingsStore } from '$lib/stores/keybindings.svelte';
  import { matchesBinding } from '$lib/utils/keybindings';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import ExperimentalBadge from '$lib/components/shared/ui/ExperimentalBadge.svelte';
  import { listRegistryWithRoots } from '$lib/ipc/workspace';
  import { type RepoRegistryEntryWithRoot, workspaceColorVar } from '$lib/types/workspace';
  import {
    createWorktreeLink, deleteWorktreeLink, renameWorktreeLink,
    addWorktreeLinkMember, removeWorktreeLinkMember, setWorktreeLinkSyncEnabled,
    setWorktreeLinkMemberSyncEnabled,
    addAliasGroup, updateAliasGroup, removeAliasGroup,
  } from '$lib/ipc/linkedWorktree';
  import type { WorktreeLink, AliasEntry, AliasGroup } from '$lib/types/linkedWorktree';
  import type { RepoRegistryEntry } from '$lib/types/workspace';

  let selectedId  = $state<string | null>(null);
  let editingName = $state(false);
  let nameDraft   = $state('');
  let creating    = $state(false);
  let createName  = $state('');
  let createNameEl: HTMLInputElement | undefined = $state();
  $effect(() => { if (creating) createNameEl?.focus(); });
  let busy        = $state(false);
  let error       = $state<string | null>(null);
  /** Sidebar collapse state — persisted in localStorage. */
  let sidebarCollapsed = $state(
    (() => { try { return localStorage.getItem('arbor:link-manager-sidebar') === 'collapsed'; } catch { return false; } })()
  );

  $effect(() => {
    try { localStorage.setItem('arbor:link-manager-sidebar', sidebarCollapsed ? 'collapsed' : 'expanded'); } catch { /* ignore */ }
  });

  // ── Derived ───────────────────────────────────────────────────────────────

  const links        = $derived(linkedWorktreesStore.links);
  const selectedLink = $derived<WorktreeLink | null>(
    links.find(l => l.id === selectedId) ?? null,
  );

  const registry: RepoRegistryEntry[] = $derived(workspacesStore.registry);
  function aliasLabel(repoId: string): string {
    const re = repoEntry(repoId);
    return re ? re.display_name : repoId.slice(0, 6);
  }

  function buildAliasItems(members: { repo_id: string }[], currentRepoId: string, idx: number): DropdownItem[] {
    return members.map(m => ({
      kind:    'item',
      id:      m.repo_id,
      label:   aliasLabel(m.repo_id),
      active:  currentRepoId === m.repo_id,
      onclick: () => {
        groupDraft[idx].repo_id = m.repo_id;
        groupDraft = [...groupDraft];
      },
    }));
  }

  function repoEntry(id: string): RepoRegistryEntry | null {
    return registry.find(r => r.id === id) ?? null;
  }

  // ── Initial selection ──────────────────────────────────────────────────────

  $effect(() => {
    if (!uiStore.linkManagerOpen) return;
    if (!links.length) {
      selectedId = null;
      return;
    }
    const initial = uiStore.linkManagerInitialId;
    if (initial && links.some(l => l.id === initial)) {
      selectedId = initial;
    } else if (!selectedId || !links.some(l => l.id === selectedId)) {
      selectedId = links[0].id;
    }
  });

  // ── Handlers ───────────────────────────────────────────────────────────────

  function close() {
    uiStore.closeLinkManager();
    creating  = false;
    editingName = false;
    error = null;
  }

  // Modal-scoped keyboard shortcuts.  Registered in the capture phase via
  // the $effect below so they run BEFORE AppShell's own window listener,
  // with `stopImmediatePropagation()` to keep the event from leaking — same
  // pattern as ConflictResolutionModal.  Otherwise Ctrl+B would toggle the
  // app's main sidebar while the user is mid-link-management.
  function handleKey(e: KeyboardEvent) {
    if (!uiStore.linkManagerOpen) return;

    if (e.key === 'Escape') {
      if (editingName) { e.preventDefault(); e.stopImmediatePropagation(); editingName = false; return; }
      if (creating)    { e.preventDefault(); e.stopImmediatePropagation(); creating = false; return; }
      e.preventDefault();
      e.stopImmediatePropagation();
      close();
      return;
    }

    if (matchesBinding(e, keybindingsStore.getBinding('toggle_sidebar'))) {
      e.preventDefault();
      e.stopImmediatePropagation();
      sidebarCollapsed = !sidebarCollapsed;
      return;
    }
  }

  $effect(() => {
    if (!uiStore.linkManagerOpen) return;
    const onKey = (e: KeyboardEvent) => handleKey(e);
    window.addEventListener('keydown', onKey, { capture: true });
    return () => window.removeEventListener('keydown', onKey, { capture: true });
  });

  // Width-collapse transition for the sidebar — drives inline styles
  // explicitly because CSS flex-basis transitions are unreliable when the
  // flex layout is rearranging at the same time.  Same approach as
  // ConflictResolutionModal.sidebarSlide.
  function sidebarSlide(node: HTMLElement, { duration = 200 }: { duration?: number } = {}) {
    const w = node.getBoundingClientRect().width;
    return {
      duration,
      easing: cubicOut,
      css: (t: number) =>
        `width: ${t * w}px; min-width: 0; margin-right: ${t * 4}px; opacity: ${t}; overflow: hidden; flex: 0 0 auto;`,
    };
  }

  function startCreate() {
    creating = true;
    createName = '';
    error = null;
  }
  async function confirmCreate() {
    if (!createName.trim()) { error = 'Name required.'; return; }
    busy = true; error = null;
    try {
      const link = await createWorktreeLink(createName.trim(), []);
      creating = false;
      selectedId = link.id;
    } catch (e) { error = `${e}`; }
    finally { busy = false; }
  }

  function startRename() {
    if (!selectedLink) return;
    nameDraft = selectedLink.name;
    editingName = true;
  }
  async function confirmRename() {
    if (!selectedLink || !nameDraft.trim()) { editingName = false; return; }
    busy = true; error = null;
    try {
      await renameWorktreeLink(selectedLink.id, nameDraft.trim());
      editingName = false;
    } catch (e) { error = `${e}`; }
    finally { busy = false; }
  }

  let confirmDeleteLink = $state<{ id: string; name: string } | null>(null);
  function deleteCurrent() {
    if (!selectedLink) return;
    confirmDeleteLink = { id: selectedLink.id, name: selectedLink.name };
  }
  async function performDeleteLink() {
    const req = confirmDeleteLink;
    confirmDeleteLink = null;
    if (!req) return;
    busy = true; error = null;
    try {
      await deleteWorktreeLink(req.id);
      if (selectedId === req.id) selectedId = null;
    } catch (e) { error = `${e}`; }
    finally { busy = false; }
  }

  async function toggleSync() {
    if (!selectedLink) return;
    busy = true; error = null;
    try {
      await setWorktreeLinkSyncEnabled(selectedLink.id, !selectedLink.sync_enabled);
    } catch (e) { error = `${e}`; }
    finally { busy = false; }
  }

  // ── Member management ─────────────────────────────────────────────────────

  let addMemberOpen   = $state(false);
  let memberPickQuery = $state('');
  let memberPickEl: HTMLInputElement | undefined = $state();
  $effect(() => { if (addMemberOpen) memberPickEl?.focus(); });
  type GroupMode = 'workspace' | 'folder' | 'flat';
  let memberGroupBy = $state<GroupMode>(
    (() => { try { const v = localStorage.getItem('arbor:link-picker-group'); return v === 'folder' || v === 'flat' ? v : 'workspace'; } catch { return 'workspace'; } })(),
  );
  $effect(() => {
    try { localStorage.setItem('arbor:link-picker-group', memberGroupBy); } catch { /* ignore */ }
  });

  /** Augmented registry (each entry plus its git common-dir).  Loaded the
   *  first time the user opens the picker; refreshed when the underlying
   *  registry changes. */
  let registryWithRoots = $state<RepoRegistryEntryWithRoot[]>([]);
  let rootsLoading      = $state(false);

  async function loadRoots() {
    rootsLoading = true;
    try { registryWithRoots = await listRegistryWithRoots(); }
    catch { registryWithRoots = []; }
    finally { rootsLoading = false; }
  }

  // Refetch whenever the modal is open AND the registry changes (the picker
  // uses this for grouping; the Members section uses it for the current
  // branch / repo path display alongside each existing member).
  $effect(() => {
    if (!uiStore.linkManagerOpen) return;
    void registry; // track
    void loadRoots();
  });

  /** Lookup helper for the Members section. */
  function repoEntryWithRoot(id: string): RepoRegistryEntryWithRoot | null {
    return registryWithRoots.find(r => r.id === id) ?? null;
  }

  /** Normalised path for de-duplication on the display side: strips trailing
   *  slashes, lowercases on Windows, swaps `\` to `/`.  Mirrors the backend
   *  comparison used in `RepoRegistry::upsert_by_path`. */
  function normalizePath(p: string): string {
    const s = p.replace(/\\/g, '/').replace(/\/+$/, '');
    return navigator.platform.toLowerCase().includes('win') ? s.toLowerCase() : s;
  }

  const availableRepos = $derived.by<RepoRegistryEntryWithRoot[]>(() => {
    // Use the augmented list when available, fall back to the bare registry
    // (no common_dir) so the picker still works during the first load.
    const source: RepoRegistryEntryWithRoot[] = registryWithRoots.length
      ? registryWithRoots
      : registry.map(r => ({ ...r, common_dir: null, current_branch: null, is_worktree: false }));

    // De-dupe entries that point at the same physical path under different
    // separators / casing.  Keep the first occurrence (registry list is
    // sorted by display_name so this is stable).
    const seen = new Map<string, RepoRegistryEntryWithRoot>();
    for (const r of source) {
      const key = normalizePath(r.path);
      if (!seen.has(key)) seen.set(key, r);
    }

    return [...seen.values()]
      .filter(r => !links.some(l => l.members.some(m => m.repo_id === r.id)))
      .filter(r =>
        !memberPickQuery ||
        r.display_name.toLowerCase().includes(memberPickQuery.toLowerCase()) ||
        r.path.toLowerCase().includes(memberPickQuery.toLowerCase())
      );
  });

  /** Last folder segment of a path (parent of the repo's own folder). */
  function parentFolder(p: string): string {
    const norm = p.replace(/\\/g, '/').replace(/\/+$/, '');
    const segments = norm.split('/').filter(Boolean);
    if (segments.length < 2) return norm || '/';
    // Drop the repo's own basename → parent path.
    return segments.slice(0, -1).join('/');
  }

  // ── Flat groups (folder + flat modes) ───────────────────────────────────
  type FlatGroup = {
    id:        string;
    label:     string;
    sublabel?: string;
    repos:     RepoRegistryEntryWithRoot[];
  };

  const flatGroups = $derived.by<FlatGroup[]>(() => {
    if (availableRepos.length === 0) return [];
    if (memberGroupBy === 'flat') {
      return [{ id: '__all__', label: 'All', repos: availableRepos }];
    }
    if (memberGroupBy === 'folder') {
      const byFolder = new Map<string, RepoRegistryEntryWithRoot[]>();
      for (const r of availableRepos) {
        const key = parentFolder(r.path);
        const arr = byFolder.get(key) ?? [];
        arr.push(r);
        byFolder.set(key, arr);
      }
      return [...byFolder.entries()]
        .sort((a, b) => a[0].localeCompare(b[0]))
        .map(([folder, repos]) => {
          const segments = folder.split('/').filter(Boolean);
          const tail     = segments[segments.length - 1] || folder;
          const head     = segments.slice(0, -1).join('/');
          return {
            id:       folder,
            label:    tail,
            sublabel: head || folder,
            repos:    repos.sort((a, b) => a.display_name.localeCompare(b.display_name)),
          };
        });
    }
    return [];
  });

  // ── Nested workspace tree (workspace mode) ──────────────────────────────
  // Two-level grouping: workspace → root.  A "root" is a single git repo
  // identified by its common_dir.  When a root has multiple worktrees they
  // appear nested under a root header (main worktree first, linked worktrees
  // below); when it has only one entry it's rendered flat — no extra header.
  type RootGroup =
    | { kind: 'multi'; id: string; label: string; sublabel?: string; mainRepoId: string | null; repos: RepoRegistryEntryWithRoot[] }
    | { kind: 'single'; id: string; repo: RepoRegistryEntryWithRoot };

  type WorkspaceTreeGroup = {
    id:        string;
    label:     string;
    colorIdx?: number;
    /** Total entries inside (across all roots), shown in the workspace count. */
    count:     number;
    items:     RootGroup[];
  };

  function isMainWorktree(r: RepoRegistryEntryWithRoot): boolean {
    if (!r.common_dir) return false;
    const mainPath = r.common_dir.replace(/\/\.git\/?$/, '');
    return normalizePath(r.path) === normalizePath(mainPath);
  }

  function buildRootItems(repos: RepoRegistryEntryWithRoot[]): RootGroup[] {
    // Group by common_dir; entries without a common_dir each form their own
    // single-entry group (we can't tell sibling worktrees from path alone).
    const byCd = new Map<string, RepoRegistryEntryWithRoot[]>();
    for (const r of repos) {
      const key = r.common_dir ?? `__solo__:${r.id}`;
      const arr = byCd.get(key) ?? [];
      arr.push(r);
      byCd.set(key, arr);
    }
    const items: RootGroup[] = [];
    for (const [key, group] of byCd.entries()) {
      if (group.length === 1) {
        items.push({ kind: 'single', id: group[0].id, repo: group[0] });
        continue;
      }
      const cd = group[0].common_dir!;
      const mainPath = cd.replace(/\/\.git\/?$/, '');
      const sorted = [...group].sort((a, b) => {
        const aMain = isMainWorktree(a);
        const bMain = isMainWorktree(b);
        if (aMain && !bMain) return -1;
        if (!aMain && bMain) return 1;
        return a.display_name.localeCompare(b.display_name);
      });
      const mainEntry = sorted.find(isMainWorktree) ?? sorted[0];
      const repoFolder = mainPath.split('/').filter(Boolean).pop() ?? cd;
      items.push({
        kind:       'multi',
        id:         key,
        label:      repoFolder,
        sublabel:   mainPath || cd,
        mainRepoId: mainEntry.id,
        repos:      sorted,
      });
    }
    // Sort items alphabetically by their visible label (main name for multi,
    // display name for single) so roots and singles intermix naturally.
    items.sort((a, b) => {
      const al = a.kind === 'multi' ? a.label : a.repo.display_name;
      const bl = b.kind === 'multi' ? b.label : b.repo.display_name;
      return al.localeCompare(bl);
    });
    return items;
  }

  const workspaceTree = $derived.by<WorkspaceTreeGroup[]>(() => {
    if (memberGroupBy !== 'workspace' || availableRepos.length === 0) return [];

    const wsList     = workspacesStore.workspaces;
    const activeWsId = workspacesStore.activeId;
    const idToWs     = new Map(wsList.map(w => [w.id, w]));

    // repo_id → workspaces it's a member of.
    const repoIdToWsIds = new Map<string, string[]>();
    for (const ws of wsList) {
      for (const repoId of ws.repo_ids) {
        const arr = repoIdToWsIds.get(repoId) ?? [];
        arr.push(ws.id);
        repoIdToWsIds.set(repoId, arr);
      }
    }

    // Sibling-by-common_dir → workspace inheritance map.
    const commonDirToWsId = new Map<string, string>();
    for (const r of availableRepos) {
      if (!r.common_dir) continue;
      const owners = repoIdToWsIds.get(r.id) ?? [];
      if (owners.length === 0) continue;
      const pick =
        (activeWsId && owners.includes(activeWsId)) ? activeWsId : owners[0];
      const existing = commonDirToWsId.get(r.common_dir);
      if (!existing || (pick === activeWsId && existing !== activeWsId)) {
        commonDirToWsId.set(r.common_dir, pick);
      }
    }

    // Bucket repos by primary workspace.  Resolution order:
    //   1. Active workspace if member.
    //   2. First workspace it's a member of.
    //   3. A sibling worktree's workspace (via common_dir inheritance).
    //   4. ORPHAN ("No workspace") — kept as a single bucket regardless of
    //      common_dir, so worktree families that lack any workspace surface
    //      together inside "No workspace" rather than in a side section.
    const ORPHAN = '__orphan__';
    const byWs = new Map<string, RepoRegistryEntryWithRoot[]>();
    for (const r of availableRepos) {
      const owners = repoIdToWsIds.get(r.id) ?? [];
      let primary: string =
        (activeWsId && owners.includes(activeWsId)) ? activeWsId :
        (owners[0] ?? '');
      if (!primary && r.common_dir) {
        primary = commonDirToWsId.get(r.common_dir) ?? '';
      }
      if (!primary) primary = ORPHAN;
      const arr = byWs.get(primary) ?? [];
      arr.push(r);
      byWs.set(primary, arr);
    }

    const out: WorkspaceTreeGroup[] = [];

    function pushWorkspace(id: string, label: string, colorIdx: number | undefined, repos: RepoRegistryEntryWithRoot[]) {
      out.push({
        id, label, colorIdx,
        count: repos.length,
        items: buildRootItems(repos),
      });
    }

    // Active workspace first.
    if (activeWsId && byWs.has(activeWsId)) {
      const ws = idToWs.get(activeWsId);
      pushWorkspace(activeWsId, ws?.name ?? 'Active workspace', ws?.color_idx, byWs.get(activeWsId) ?? []);
      byWs.delete(activeWsId);
    }
    // Other named workspaces.
    const others = [...byWs.entries()]
      .filter(([id]) => idToWs.has(id))
      .sort((a, b) => (idToWs.get(a[0])?.name ?? '').localeCompare(idToWs.get(b[0])?.name ?? ''));
    for (const [id, repos] of others) {
      const ws = idToWs.get(id);
      pushWorkspace(id, ws?.name ?? id, ws?.color_idx, repos);
      byWs.delete(id);
    }
    // Truly unaffiliated entries — `buildRootItems` still groups linked
    // worktrees together as a multi-root inside the bucket.
    if (byWs.has(ORPHAN)) {
      pushWorkspace(ORPHAN, 'No workspace', undefined, byWs.get(ORPHAN) ?? []);
    }
    return out;
  });

  async function addMember(r: RepoRegistryEntry) {
    if (!selectedLink) return;
    busy = true; error = null;
    try {
      await addWorktreeLinkMember(selectedLink.id, r.id);
      addMemberOpen = false;
      memberPickQuery = '';
    } catch (e) { error = `${e}`; }
    finally { busy = false; }
  }
  async function removeMember(repoId: string) {
    if (!selectedLink) return;
    busy = true; error = null;
    try {
      await removeWorktreeLinkMember(selectedLink.id, repoId);
    } catch (e) { error = `${e}`; }
    finally { busy = false; }
  }

  async function toggleMemberSync(repoId: string, current: boolean) {
    if (!selectedLink) return;
    busy = true; error = null;
    try {
      await setWorktreeLinkMemberSyncEnabled(selectedLink.id, repoId, !current);
    } catch (e) { error = `${e}`; }
    finally { busy = false; }
  }

  // ── Alias group management ────────────────────────────────────────────────

  let editingGroupId = $state<string | null>(null);
  let groupDraft     = $state<AliasEntry[]>([]);

  function startNewGroup() {
    if (!selectedLink) return;
    editingGroupId = '__new__';
    groupDraft = selectedLink.members.slice(0, 2).map(m => ({ repo_id: m.repo_id, branch: '' }));
  }
  function startEditGroup(g: AliasGroup) {
    editingGroupId = g.id;
    groupDraft = g.members.map(e => ({ ...e }));
  }
  function cancelEditGroup() {
    editingGroupId = null;
    groupDraft = [];
  }
  function addGroupMemberRow() {
    if (!selectedLink) return;
    const used = new Set(groupDraft.map(e => e.repo_id));
    const pick = selectedLink.members.find(m => !used.has(m.repo_id));
    if (pick) groupDraft = [...groupDraft, { repo_id: pick.repo_id, branch: '' }];
  }
  function removeGroupMemberRow(i: number) {
    groupDraft = groupDraft.filter((_, idx) => idx !== i);
  }
  async function saveGroup() {
    if (!selectedLink) return;
    if (groupDraft.length < 2) { error = 'Alias group needs at least 2 members.'; return; }
    if (groupDraft.some(e => !e.branch.trim())) { error = 'Every entry needs a branch name.'; return; }
    busy = true; error = null;
    try {
      if (editingGroupId === '__new__') {
        await addAliasGroup(selectedLink.id, groupDraft);
      } else if (editingGroupId) {
        await updateAliasGroup(selectedLink.id, editingGroupId, groupDraft);
      }
      editingGroupId = null;
      groupDraft = [];
    } catch (e) { error = `${e}`; }
    finally { busy = false; }
  }
  let confirmDropGroup = $state<{ linkId: string; groupId: string } | null>(null);
  function dropGroup(g: AliasGroup) {
    if (!selectedLink) return;
    confirmDropGroup = { linkId: selectedLink.id, groupId: g.id };
  }
  async function performDropGroup() {
    const req = confirmDropGroup;
    confirmDropGroup = null;
    if (!req) return;
    busy = true; error = null;
    try {
      await removeAliasGroup(req.linkId, req.groupId);
    } catch (e) { error = `${e}`; }
    finally { busy = false; }
  }

  function formatTimeAgo(ts: number): string {
    const diff = Date.now() / 1000 - ts;
    if (diff < 60) return 'just now';
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    return `${Math.floor(diff / 86400)}d ago`;
  }
</script>

{#if uiStore.linkManagerOpen}
  <Modal onClose={close} width="86vw" height="78vh" padBody={false} ariaLabel="Manage Linked Worktrees">
    {#snippet header()}
      <ModalHeader onClose={close}>
        <ModalSidebarToggle
          collapsed={sidebarCollapsed}
          onToggle={() => sidebarCollapsed = !sidebarCollapsed}
          label={(sidebarCollapsed ? 'Show links list' : 'Hide links list') + ' (Ctrl+B)'}
        />
        <Layers size={13} class="muted-icon" />
        <span class="modal-title">Linked Worktrees</span>
        <ExperimentalBadge />
        {#if selectedLink}
          <span class="header-meta">
            <span class="meta-dot"></span>
            {selectedLink.members.length} member{selectedLink.members.length === 1 ? '' : 's'}
          </span>
        {/if}
      </ModalHeader>
    {/snippet}

    <div class="split-layout">
          <!-- LEFT: links list -->
          {#if !sidebarCollapsed}
            <aside class="sidebar" transition:sidebarSlide={{ duration: animStore.dPanel }}>
              <div class="sidebar-header">
                <span class="sidebar-title">Links</span>
                <span class="sidebar-count">{links.length}</span>
                <button class="icon-btn" onclick={startCreate} use:tooltip={'New worktree link'}>
                  <Plus size={13} />
                </button>
              </div>

              {#if creating}
                <div class="create-row">
                  <input
                    class="input"
                    placeholder="Link name…"
                    bind:value={createName}
                    onkeydown={(e) => { if (e.key === 'Enter') confirmCreate(); else if (e.key === 'Escape') creating = false; }}
                    bind:this={createNameEl}
                  />
                  <button class="icon-btn primary" onclick={confirmCreate} disabled={busy} use:tooltip={'Create'}><Check size={13}/></button>
                  <button class="icon-btn" onclick={() => { creating = false; }} use:tooltip={'Cancel'}><X size={13}/></button>
                </div>
              {/if}

              <div class="sidebar-body">
                {#if links.length === 0 && !creating}
                  <div class="empty-row">
                    <Link2 size={22} />
                    <p>No links yet</p>
                    <button class="btn-link" onclick={startCreate}>Create your first link</button>
                  </div>
                {:else}
                  <ul class="link-list">
                    {#each links as l}
                      <li>
                        <button
                          class="link-card"
                          class:active={selectedId === l.id}
                          onclick={() => { selectedId = l.id; }}
                        >
                          <span class="link-card-icon">
                            <Layers size={12} />
                          </span>
                          <span class="link-card-name">{l.name}</span>
                          <span class="link-card-meta">
                            <span class="link-card-count">{l.members.length}</span>
                            {#if !l.sync_enabled}<span class="off-pill" use:tooltip={'Sync disabled'}>OFF</span>{/if}
                          </span>
                        </button>
                      </li>
                    {/each}
                  </ul>
                {/if}
              </div>
            </aside>
          {/if}

          <!-- RIGHT: detail -->
          <section class="detail">
            {#if !selectedLink}
              <div class="empty-state">
                <Layers size={42} />
                <p class="empty-title">No link selected</p>
                <p class="empty-sub">Pick a link from the list or create a new one.</p>
                <button class="btn-primary" onclick={startCreate}>
                  <Plus size={12} /> New link
                </button>
              </div>
            {:else}
              <!-- Detail header -->
              <div class="detail-header">
                {#if editingName}
                  <input
                    class="input title-input"
                    bind:value={nameDraft}
                    onkeydown={(e) => { if (e.key === 'Enter') confirmRename(); else if (e.key === 'Escape') editingName = false; }}
                  />
                  <button class="icon-btn primary" onclick={confirmRename} use:tooltip={'Save'}><Check size={13}/></button>
                  <button class="icon-btn" onclick={() => { editingName = false; }} use:tooltip={'Cancel'}><X size={13}/></button>
                {:else}
                  <h2 class="link-title">{selectedLink.name}</h2>
                  <button class="icon-btn subtle" onclick={startRename} use:tooltip={'Rename'}><Edit3 size={12}/></button>
                {/if}
                <span class="header-spacer"></span>
                <button
                  class="sync-toggle"
                  class:on={selectedLink.sync_enabled}
                  onclick={toggleSync}
                  use:tooltip={selectedLink.sync_enabled ? 'Sync enabled — click to disable' : 'Sync disabled — click to enable'}
                >
                  <Power size={11}/>
                  <span>{selectedLink.sync_enabled ? 'Sync on' : 'Sync off'}</span>
                </button>
                <button class="icon-btn danger" onclick={deleteCurrent} use:tooltip={'Delete link'}>
                  <Trash2 size={12}/>
                </button>
              </div>

              {#if error}<div class="error-msg">{error}</div>{/if}

              <!-- Sections -->
              <div class="detail-body">
                <!-- Members -->
                <section class="card">
                  <div class="card-header">
                    <Folder size={11} />
                    <h3>Members</h3>
                    <span class="card-count">{selectedLink.members.length}</span>
                    <span class="header-spacer"></span>
                    <button class="btn-ghost" onclick={() => { addMemberOpen = !addMemberOpen; memberPickQuery = ''; }}>
                      <Plus size={11}/> Add member
                    </button>
                  </div>

                  {#if addMemberOpen}
                    <div class="picker">
                      <div class="picker-toolbar">
                        <input
                          class="input"
                          placeholder="Search registry…"
                          bind:value={memberPickQuery}
                          bind:this={memberPickEl}
                          onkeydown={(e) => { if (e.key === 'Escape') addMemberOpen = false; }}
                        />
                        <div class="seg-group" role="tablist" aria-label="Group repos by">
                          <button
                            class="seg"
                            class:active={memberGroupBy === 'workspace'}
                            onclick={() => memberGroupBy = 'workspace'}
                            use:tooltip={'Group by workspace'}
                            role="tab"
                            aria-selected={memberGroupBy === 'workspace'}
                          >Workspace</button>
                          <button
                            class="seg"
                            class:active={memberGroupBy === 'folder'}
                            onclick={() => memberGroupBy = 'folder'}
                            use:tooltip={'Group by parent folder'}
                            role="tab"
                            aria-selected={memberGroupBy === 'folder'}
                          >Folder</button>
                          <button
                            class="seg"
                            class:active={memberGroupBy === 'flat'}
                            onclick={() => memberGroupBy = 'flat'}
                            use:tooltip={'No grouping'}
                            role="tab"
                            aria-selected={memberGroupBy === 'flat'}
                          >Flat</button>
                        </div>
                      </div>

                      {#if availableRepos.length === 0}
                        <div class="picker-empty">No repos available.</div>
                      {:else if memberGroupBy === 'workspace'}
                        <!-- Two-level nested tree: workspace > root > worktrees -->
                        <div class="picker-scroll">
                          {#each workspaceTree as ws (ws.id)}
                            <div class="picker-group">
                              <div class="picker-group-header lvl-1">
                                {#if ws.colorIdx !== undefined}
                                  <Monogram name={ws.label} color={workspaceColorVar(ws.colorIdx)} size={12} />
                                {:else}
                                  <Folder size={11} class="muted-icon"/>
                                {/if}
                                <span class="picker-group-label">{ws.label}</span>
                                <span class="picker-group-count">{ws.count}</span>
                              </div>

                              <div class="picker-group-body">
                                {#each ws.items as item (item.id)}
                                  {#if item.kind === 'single'}
                                    <ul class="picker-list">
                                      <li>
                                        <button class="picker-row" onclick={() => addMember(item.repo)}>
                                          <Folder size={11}/>
                                          <span class="picker-name">{item.repo.display_name}</span>
                                          <span class="picker-path" use:tooltip={item.repo.path}>{item.repo.path}</span>
                                          <Plus size={11} class="picker-add" />
                                        </button>
                                      </li>
                                    </ul>
                                  {:else}
                                    <div class="root-group">
                                      <div class="picker-group-header lvl-2">
                                        <Folder size={10} class="muted-icon"/>
                                        <span class="picker-group-label">{item.label}</span>
                                        {#if item.sublabel}
                                          <span class="picker-group-sublabel" use:tooltip={item.sublabel}>{item.sublabel}</span>
                                        {/if}
                                        <span class="picker-group-count">{item.repos.length}</span>
                                      </div>
                                      <ul class="picker-list nested">
                                        {#each item.repos as r (r.id)}
                                          <li>
                                            <button class="picker-row" onclick={() => addMember(r)}>
                                              <Folder size={11}/>
                                              <span class="picker-name">{r.display_name}</span>
                                              {#if r.id === item.mainRepoId}
                                                <span class="main-chip" use:tooltip={'Main worktree'}>main</span>
                                              {:else}
                                                <span class="linked-chip" use:tooltip={'Linked worktree'}>linked</span>
                                              {/if}
                                              <span class="picker-path" use:tooltip={r.path}>{r.path}</span>
                                              <Plus size={11} class="picker-add" />
                                            </button>
                                          </li>
                                        {/each}
                                      </ul>
                                    </div>
                                  {/if}
                                {/each}
                              </div>
                            </div>
                          {/each}
                        </div>
                      {:else}
                        <!-- Flat / folder modes — single level groups -->
                        <div class="picker-scroll">
                          {#each flatGroups as g (g.id)}
                            <div class="picker-group">
                              {#if memberGroupBy !== 'flat'}
                                <div class="picker-group-header lvl-1">
                                  <Folder size={11} class="muted-icon"/>
                                  <span class="picker-group-label">{g.label}</span>
                                  {#if g.sublabel}
                                    <span class="picker-group-sublabel" use:tooltip={g.sublabel}>{g.sublabel}</span>
                                  {/if}
                                  <span class="picker-group-count">{g.repos.length}</span>
                                </div>
                              {/if}
                              <ul class="picker-list">
                                {#each g.repos as r (r.id)}
                                  <li>
                                    <button class="picker-row" onclick={() => addMember(r)}>
                                      <Folder size={11}/>
                                      <span class="picker-name">{r.display_name}</span>
                                      <span class="picker-path" use:tooltip={r.path}>{r.path}</span>
                                      <Plus size={11} class="picker-add" />
                                    </button>
                                  </li>
                                {/each}
                              </ul>
                            </div>
                          {/each}
                        </div>
                      {/if}
                    </div>
                  {/if}

                  {#if selectedLink.members.length === 0}
                    <div class="card-empty">No members yet — add at least one repo.</div>
                  {:else}
                    <ul class="member-list">
                      {#each selectedLink.members as m}
                        {@const r        = repoEntry(m.repo_id)}
                        {@const wr       = repoEntryWithRoot(m.repo_id)}
                        {@const expected = linkedWorktreesStore.expectedBranchFor(selectedLink, m.repo_id)}
                        {@const current  = wr?.current_branch ?? null}
                        {@const synced   = !!expected && !!current && expected === current}
                        {@const drift    = !!expected && !!current && expected !== current}
                        <li class="member-row" class:broken={!r} class:sync-off={!m.sync_enabled}>
                          <span class="member-icon"><Folder size={11}/></span>
                          {#if r}
                            <span class="member-name">{r.display_name}</span>
                            {#if current}
                              <span class="member-branch" use:tooltip={'Current branch'}>
                                <GitBranch size={10}/>
                                <span>{current}</span>
                              </span>
                            {/if}
                            {#if !m.sync_enabled}
                              <span class="status-pill status-off" use:tooltip={'Sync disabled for this member'}>off</span>
                            {:else if synced}
                              <span class="status-pill status-synced" use:tooltip={'On the link\'s last-synced branch'}>synced</span>
                            {:else if drift}
                              <span class="status-pill status-drift" use:tooltip={`Expected '${expected}' for the link's last sync target`}>out of sync</span>
                            {/if}
                            <span class="member-path" use:tooltip={r.path}>{r.path}</span>
                          {:else}
                            <span class="member-name">Unknown repo</span>
                            <span class="member-path muted">{m.repo_id}</span>
                            <span class="badge-broken">
                              <AlertCircle size={10}/> broken
                            </span>
                          {/if}
                          <span class="header-spacer"></span>
                          <button
                            class="member-sync-toggle"
                            class:on={m.sync_enabled}
                            onclick={() => toggleMemberSync(m.repo_id, m.sync_enabled)}
                            use:tooltip={m.sync_enabled ? 'Sync enabled — click to disable for this member' : 'Sync disabled — click to enable for this member'}
                          >
                            <Power size={10}/>
                          </button>
                          <button class="icon-btn danger subtle" onclick={() => removeMember(m.repo_id)} use:tooltip={'Remove'}>
                            <Trash2 size={11}/>
                          </button>
                        </li>
                      {/each}
                    </ul>
                  {/if}
                </section>

                <!-- Branch Aliases -->
                <section class="card">
                  <div class="card-header">
                    <Link2 size={11} />
                    <h3>Branch Aliases</h3>
                    <span class="card-count">{selectedLink.alias_groups.length}</span>
                    <span class="header-spacer"></span>
                    <button
                      class="btn-ghost"
                      disabled={selectedLink.members.length < 2}
                      onclick={startNewGroup}
                    >
                      <Plus size={11}/> New alias group
                    </button>
                  </div>

                  {#if selectedLink.members.length < 2}
                    <div class="card-empty">Need at least 2 members to define aliases.</div>
                  {:else if selectedLink.alias_groups.length === 0 && editingGroupId !== '__new__'}
                    <div class="card-empty">No aliases — branches map by identical name across members.</div>
                  {/if}

                  {#each selectedLink.alias_groups as g}
                    {#if editingGroupId === g.id}
                      <div class="alias-card editing">
                        {#each groupDraft as entry, i}
                          <div class="alias-row">
                            <Dropdown
                              position="fixed"
                              direction="down"
                              items={buildAliasItems(selectedLink.members, entry.repo_id, i)}
                            >
                              {#snippet trigger({ open, toggle })}
                                <button
                                  class="input alias-select"
                                  onclick={toggle}
                                  type="button"
                                  aria-haspopup="listbox"
                                  aria-expanded={open}
                                >
                                  <span class="alias-select-label">{aliasLabel(entry.repo_id)}</span>
                                  <ChevronDown size={11} />
                                </button>
                              {/snippet}
                            </Dropdown>
                            <GitBranch size={10} class="muted-icon"/>
                            <input class="input alias-branch" placeholder="branch name" bind:value={entry.branch} />
                            <button class="icon-btn subtle" onclick={() => removeGroupMemberRow(i)} use:tooltip={'Remove row'}>
                              <X size={11}/>
                            </button>
                          </div>
                        {/each}
                        <div class="alias-actions">
                          <button class="btn-ghost" onclick={addGroupMemberRow}>
                            <Plus size={11}/> Add row
                          </button>
                          <span class="header-spacer"></span>
                          <button class="btn-ghost" onclick={cancelEditGroup}>Cancel</button>
                          <button class="btn-primary" onclick={saveGroup} disabled={busy}>Save</button>
                        </div>
                      </div>
                    {:else}
                      <div class="alias-card">
                        <div class="alias-summary">
                          {#each g.members as e, i}
                            {@const re = repoEntry(e.repo_id)}
                            <span class="alias-chip">
                              <Folder size={9}/>
                              <span class="alias-chip-repo">{re ? re.display_name : e.repo_id.slice(0, 6)}</span>
                              <span class="alias-chip-sep">:</span>
                              <strong class="alias-chip-branch">{e.branch}</strong>
                            </span>
                            {#if i < g.members.length - 1}
                              <span class="alias-link"><Link2 size={9}/></span>
                            {/if}
                          {/each}
                        </div>
                        <div class="alias-row-actions">
                          <button class="icon-btn subtle" onclick={() => startEditGroup(g)} use:tooltip={'Edit'}>
                            <Edit3 size={11}/>
                          </button>
                          <button class="icon-btn danger subtle" onclick={() => dropGroup(g)} use:tooltip={'Remove'}>
                            <Trash2 size={11}/>
                          </button>
                        </div>
                      </div>
                    {/if}
                  {/each}

                  {#if editingGroupId === '__new__'}
                    <div class="alias-card editing">
                      {#each groupDraft as entry, i}
                        <div class="alias-row">
                          <select class="input alias-select" bind:value={entry.repo_id}>
                            {#each selectedLink.members as m}
                              {@const re = repoEntry(m.repo_id)}
                              <option value={m.repo_id}>{re ? re.display_name : m.repo_id.slice(0, 6)}</option>
                            {/each}
                          </select>
                          <GitBranch size={10} class="muted-icon"/>
                          <input class="input alias-branch" placeholder="branch name" bind:value={entry.branch} />
                          <button class="icon-btn subtle" onclick={() => removeGroupMemberRow(i)} title="Remove row">
                            <X size={11}/>
                          </button>
                        </div>
                      {/each}
                      <div class="alias-actions">
                        <button class="btn-ghost" onclick={addGroupMemberRow}>
                          <Plus size={11}/> Add row
                        </button>
                        <span class="header-spacer"></span>
                        <button class="btn-ghost" onclick={cancelEditGroup}>Cancel</button>
                        <button class="btn-primary" onclick={saveGroup} disabled={busy}>Create</button>
                      </div>
                    </div>
                  {/if}
                </section>

                <!-- Last sync -->
                <section class="card">
                  <div class="card-header">
                    <RefreshCw size={11} />
                    <h3>Last Sync</h3>
                  </div>
                  {#if selectedLink.last_sync_target}
                    {@const t = selectedLink.last_sync_target}
                    {@const re = repoEntry(t.initiator_repo_id)}
                    <div class="last-sync-row">
                      <span class="ls-label">Target</span>
                      <span class="ls-value branch"><GitBranch size={11}/> {t.branch}</span>
                    </div>
                    <div class="last-sync-row">
                      <span class="ls-label">Initiator</span>
                      <span class="ls-value">{re ? re.display_name : t.initiator_repo_id.slice(0, 8)}</span>
                    </div>
                    <div class="last-sync-row">
                      <span class="ls-label">When</span>
                      <span class="ls-value muted">{formatTimeAgo(t.timestamp)} · {new Date(t.timestamp * 1000).toLocaleString()}</span>
                    </div>
                  {:else}
                    <div class="card-empty">No sync yet — first checkout on a member will set the target.</div>
                  {/if}
                </section>
              </div>
            {/if}
          </section>
        </div>
  </Modal>
{/if}

{#if confirmDeleteLink}
  <ConfirmModal
    title="Delete worktree link"
    message={`Delete worktree link "${confirmDeleteLink.name}"?`}
    detail="This cannot be undone."
    variant="danger"
    confirmLabel="Delete"
    onCancel={() => confirmDeleteLink = null}
    onConfirm={performDeleteLink}
  />
{/if}

{#if confirmDropGroup}
  <ConfirmModal
    title="Remove alias group"
    message="Remove this alias group?"
    variant="danger"
    confirmLabel="Remove"
    onCancel={() => confirmDropGroup = null}
    onConfirm={performDropGroup}
  />
{/if}

<style>
  :global(.muted-icon) { color: var(--text-muted); flex-shrink: 0; }

  .header-meta {
    display: inline-flex; align-items: center; gap: 6px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
  .meta-dot {
    width: 3px; height: 3px; border-radius: 50%;
    background: var(--text-disabled);
  }
  /* ── Body / split layout ── */
  .split-layout {
    display: flex;
    height: 100%;
    overflow: hidden;
    background: var(--bg-elevated);
    padding: 4px;
    gap: 4px;
  }

  /* ── Sidebar (links list) ── */
  .sidebar {
    width: 240px;
    min-width: 200px;
    flex-shrink: 0;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
    display: flex; flex-direction: column;
    overflow: hidden;
  }
  .sidebar-header {
    display: flex; align-items: center; gap: 6px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .sidebar-title {
    flex: 1;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
  }
  .sidebar-count {
    font-size: var(--font-size-xs);
    color: var(--text-disabled);
    background: var(--bg-overlay);
    border-radius: 999px;
    padding: 0 6px;
    line-height: 16px;
  }

  .create-row {
    display: flex; gap: 4px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .create-row .input { flex: 1; }

  .sidebar-body {
    flex: 1; overflow-y: auto;
    padding: 6px 8px;
    display: flex; flex-direction: column; gap: 4px;
  }

  .empty-row {
    display: flex; flex-direction: column; align-items: center;
    gap: 6px; padding: 24px 12px;
    color: var(--text-muted);
    font-size: var(--font-size-xs);
  }
  .empty-row p { margin: 0; }
  .btn-link {
    background: transparent; border: none;
    color: var(--accent);
    font-size: var(--font-size-xs);
    cursor: pointer;
    padding: 0;
    text-decoration: underline;
    text-decoration-thickness: 1px;
    text-underline-offset: 2px;
  }
  .btn-link:hover { color: var(--accent-hover); }

  .link-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  .link-card {
    display: flex; align-items: center; gap: 7px;
    width: 100%;
    padding: 8px 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    cursor: pointer;
    text-align: left;
    color: var(--text-secondary);
    transition: background var(--transition-fast), border-color var(--transition-fast),
                box-shadow var(--transition-fast), color var(--transition-fast);
  }
  .link-card:hover {
    background: var(--bg-overlay);
    border-color: var(--border);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.15);
    color: var(--text-primary);
  }
  .link-card.active {
    background: var(--accent-subtle);
    border-color: color-mix(in srgb, var(--accent) 55%, transparent);
    color: var(--accent);
  }
  .link-card-icon { display: flex; flex-shrink: 0; opacity: 0.85; }
  .link-card-name {
    flex: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-size: var(--font-size-sm);
    font-weight: 500;
  }
  .link-card-meta { display: flex; align-items: center; gap: 5px; flex-shrink: 0; }
  .link-card-count {
    font-size: 10px;
    color: var(--text-disabled);
    background: var(--bg-overlay);
    border-radius: 999px;
    padding: 0 6px;
    line-height: 15px;
  }
  .link-card.active .link-card-count {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .off-pill {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.04em;
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    background: rgba(220,50,50,0.15);
    color: var(--error);
  }

  /* ── Detail panel ── */
  .detail {
    flex: 1; min-width: 0;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
    display: flex; flex-direction: column;
    overflow: hidden;
  }
  .empty-state {
    flex: 1;
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    gap: 10px;
    color: var(--text-muted);
  }
  .empty-state :global(svg) { color: var(--text-disabled); }
  .empty-title { margin: 4px 0 0; font-size: var(--font-size-sm); font-weight: 600; color: var(--text-secondary); }
  .empty-sub   { margin: 0; font-size: var(--font-size-xs); }

  .detail-header {
    display: flex; align-items: center; gap: 8px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .link-title {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .title-input { font-size: 14px; font-weight: 600; }

  .sync-toggle {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 3px 9px;
    height: 22px;
    border-radius: 11px;
    border: 1px solid var(--border);
    background: var(--bg-overlay);
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: all var(--transition-fast);
  }
  .sync-toggle:hover { background: var(--bg-hover); color: var(--text-primary); }
  .sync-toggle.on {
    background: color-mix(in srgb, var(--success) 15%, transparent);
    border-color: color-mix(in srgb, var(--success) 45%, transparent);
    color: var(--success);
  }
  .sync-toggle.on:hover { background: color-mix(in srgb, var(--success) 25%, transparent); }

  .error-msg {
    margin: 10px 16px 0;
    padding: 7px 10px;
    background: rgba(220,50,50,0.12);
    border: 1px solid rgba(220,50,50,0.25);
    border-radius: var(--radius-sm);
    font-size: 11.5px;
    color: var(--error);
  }

  /* ── Detail body (scrollable) ── */
  .detail-body {
    flex: 1; overflow-y: auto;
    padding: 14px 16px;
    display: flex; flex-direction: column;
    gap: 12px;
  }

  /* ── Section card ── */
  .card {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .card-header {
    display: flex; align-items: center; gap: 6px;
    padding: 9px 12px;
    border-bottom: 1px solid var(--border-subtle);
    background: color-mix(in srgb, var(--bg-base) 50%, transparent);
  }
  .card-header h3 {
    margin: 0;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-secondary);
  }
  .card-count {
    font-size: var(--font-size-xs);
    color: var(--text-disabled);
    background: var(--bg-overlay);
    border-radius: 999px;
    padding: 0 6px;
    line-height: 16px;
  }
  .card-empty {
    padding: 16px;
    text-align: center;
    color: var(--text-muted);
    font-size: 12px;
    font-style: italic;
  }

  /* ── Member list ── */
  .member-list {
    list-style: none;
    margin: 0; padding: 6px;
    display: flex; flex-direction: column;
    gap: 4px;
  }
  .member-row {
    display: flex; align-items: center; gap: 8px;
    padding: 7px 10px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    font-size: 12px;
    color: var(--text-secondary);
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .member-row:hover {
    background: var(--bg-hover);
    border-color: var(--border);
  }
  .member-row.broken {
    background: rgba(220,50,50,0.06);
    border-color: rgba(220,50,50,0.20);
  }
  .member-row.sync-off {
    opacity: 0.62;
  }
  .member-row.sync-off .member-name { color: var(--text-secondary); }
  .member-icon { color: var(--text-disabled); flex-shrink: 0; }
  .member-name { font-weight: 500; color: var(--text-primary); flex-shrink: 0; }
  .member-branch {
    display: inline-flex; align-items: center; gap: 3px;
    flex-shrink: 0;
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--accent-subtle);
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
    font-size: 10.5px;
    color: var(--accent);
    font-family: var(--font-code);
    max-width: 200px;
  }
  .member-branch :global(svg) { color: var(--accent); }
  .member-branch span {
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .member-path {
    font-family: var(--font-code);
    font-size: 10.5px;
    color: var(--text-muted);
    flex: 1; min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
  }
  .member-sync-toggle {
    display: inline-flex; align-items: center; justify-content: center;
    width: 22px; height: 22px;
    border: 1px solid var(--border);
    background: var(--bg-overlay);
    color: var(--text-muted);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: all var(--transition-fast);
    flex-shrink: 0;
  }
  .member-sync-toggle:hover { background: var(--bg-hover); color: var(--text-primary); }
  .member-sync-toggle.on {
    background: color-mix(in srgb, var(--success) 15%, transparent);
    border-color: color-mix(in srgb, var(--success) 45%, transparent);
    color: var(--success);
  }
  .member-sync-toggle.on:hover { background: color-mix(in srgb, var(--success) 25%, transparent); }

  /* Status pill: synced / out-of-sync / off */
  .status-pill {
    flex-shrink: 0;
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 1px 6px;
    border-radius: 999px;
    line-height: 14px;
  }
  .status-synced {
    background: color-mix(in srgb, var(--success) 18%, transparent);
    color: var(--success);
    border: 1px solid color-mix(in srgb, var(--success) 35%, transparent);
  }
  .status-drift {
    background: color-mix(in srgb, var(--warning) 18%, transparent);
    color: var(--warning);
    border: 1px solid color-mix(in srgb, var(--warning) 35%, transparent);
  }
  .status-off {
    background: var(--bg-overlay);
    color: var(--text-muted);
    border: 1px solid var(--border-subtle);
  }
  .member-path.muted { color: var(--text-disabled); }
  .badge-broken {
    display: inline-flex; align-items: center; gap: 3px;
    padding: 1px 6px;
    border-radius: 9px;
    background: rgba(220,50,50,0.18);
    color: var(--error);
    font-size: 10px;
    flex-shrink: 0;
  }

  /* ── Picker ── */
  .picker {
    margin: 6px;
    padding: 6px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    display: flex; flex-direction: column; gap: 6px;
  }
  .picker-toolbar {
    display: flex; align-items: center; gap: 6px;
  }
  .picker-toolbar .input { flex: 1; }

  .seg-group {
    display: inline-flex;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 2px;
    gap: 2px;
    flex-shrink: 0;
  }
  .seg {
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    padding: 3px 9px;
    font-size: 10.5px;
    font-weight: 500;
    color: var(--text-muted);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .seg:hover { color: var(--text-primary); }
  .seg.active {
    background: var(--bg-base);
    color: var(--accent);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.18);
  }

  .picker-empty {
    padding: 8px 10px;
    font-size: 11.5px;
    color: var(--text-muted);
    font-style: italic;
    text-align: center;
  }

  .picker-scroll {
    max-height: 280px;
    overflow-y: auto;
    display: flex; flex-direction: column;
    gap: 8px;
  }
  .picker-group {
    display: flex; flex-direction: column;
    gap: 2px;
  }
  .picker-group-header {
    display: flex; align-items: center; gap: 6px;
    background: var(--bg-base);
  }
  /* Workspace level — top, prominent, sticky to top of scroll. */
  .picker-group-header.lvl-1 {
    padding: 6px 6px 4px;
    border-bottom: 1px solid var(--border-subtle);
    margin-bottom: 2px;
    position: sticky;
    top: 0;
    z-index: 2;
  }
  /* Root level — nested under a workspace.  Smaller, sticky below lvl-1. */
  .picker-group-header.lvl-2 {
    padding: 4px 6px 4px 16px;
    margin: 4px 0 2px;
    color: var(--text-muted);
    background: var(--bg-base);
    position: sticky;
    /* Sit just below the lvl-1 header (≈25px tall). */
    top: 25px;
    z-index: 1;
    border-bottom: 1px dashed var(--border-subtle);
  }
  .picker-group-label {
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-secondary);
  }
  .picker-group-header.lvl-2 .picker-group-label {
    text-transform: none;
    letter-spacing: 0;
    font-size: 11px;
    color: var(--text-secondary);
  }
  .picker-group-sublabel {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-disabled);
    flex: 1; min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
  }
  .picker-group-count {
    margin-left: auto;
    font-size: 10px;
    color: var(--text-disabled);
    background: var(--bg-overlay);
    border-radius: 999px;
    padding: 0 6px;
    line-height: 15px;
    flex-shrink: 0;
  }

  /* Workspace body — small left-padding so child rows visually nest. */
  .picker-group-body {
    display: flex; flex-direction: column;
    gap: 2px;
    padding-left: 4px;
  }
  /* Root group inside a workspace. */
  .root-group {
    display: flex; flex-direction: column;
    gap: 2px;
  }

  .picker-list {
    list-style: none; margin: 0; padding: 0;
    display: flex; flex-direction: column; gap: 3px;
  }
  /* Nested worktree list under a root header — extra indent + connector hint. */
  .picker-list.nested {
    padding-left: 12px;
    border-left: 1px solid var(--border-subtle);
    margin-left: 8px;
  }

  /* Inline chips next to the worktree name. */
  .main-chip, .linked-chip {
    flex-shrink: 0;
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 1px 5px;
    border-radius: var(--radius-sm);
  }
  .main-chip {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
  }
  .linked-chip {
    background: var(--bg-overlay);
    color: var(--text-muted);
  }
  .picker-row {
    width: 100%;
    display: flex; align-items: center; gap: 7px;
    padding: 6px 8px;
    border: 1px solid transparent;
    background: transparent;
    border-radius: var(--radius-sm);
    font-size: 12px;
    color: var(--text-secondary);
    cursor: pointer;
    text-align: left;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .picker-row:hover {
    background: var(--bg-hover);
    border-color: var(--border-subtle);
    color: var(--text-primary);
  }
  .picker-name { font-weight: 500; flex-shrink: 0; }
  .picker-path {
    font-family: var(--font-code);
    font-size: 10.5px;
    color: var(--text-muted);
    flex: 1; min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
  }
  /* ── Alias card ── */
  .alias-card {
    margin: 6px;
    padding: 8px 10px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    display: flex; align-items: center; gap: 8px;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .alias-card.editing {
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
    background: var(--bg-overlay);
    border-color: var(--border);
  }
  .alias-summary {
    flex: 1; min-width: 0;
    display: flex; flex-wrap: wrap; align-items: center;
    gap: 6px;
  }
  .alias-chip {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--accent-subtle);
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
    font-size: 11px;
  }
  .alias-chip-repo { color: var(--text-secondary); font-weight: 500; }
  .alias-chip-sep  { color: var(--text-disabled); }
  .alias-chip-branch { color: var(--accent); font-family: var(--font-code); font-size: 10.5px; }
  .alias-link {
    display: inline-flex; align-items: center;
    color: var(--text-disabled);
  }
  .alias-row-actions { display: flex; gap: 2px; flex-shrink: 0; }
  .alias-row {
    display: flex; align-items: center; gap: 6px;
  }
  .alias-select {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    width: auto;
    min-width: 110px;
    cursor: pointer;
    text-align: left;
  }
  .alias-select-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .alias-branch { flex: 1; }
  .alias-actions {
    display: flex; align-items: center; gap: 6px;
    margin-top: 2px;
  }

  /* ── Last sync ── */
  .last-sync-row {
    display: flex; align-items: center; gap: 10px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: 12px;
  }
  .last-sync-row:last-child { border-bottom: none; }
  .ls-label {
    width: 70px;
    flex-shrink: 0;
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
  }
  .ls-value {
    color: var(--text-primary);
    display: inline-flex; align-items: center; gap: 4px;
  }
  .ls-value.muted { color: var(--text-muted); font-size: 11px; }
  .ls-value.branch :global(svg) { color: var(--accent); }

  /* ── Inputs / buttons ── */
  .input {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 5px 9px;
    font-size: 12px;
    color: var(--text-primary);
    outline: none;
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
    font-family: inherit;
    box-sizing: border-box;
  }
  .input:focus {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 18%, transparent);
  }

  .icon-btn {
    display: inline-flex; align-items: center; justify-content: center;
    width: 24px; height: 24px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
    flex-shrink: 0;
  }
  .icon-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .icon-btn:disabled { opacity: 0.4; pointer-events: none; }
  .icon-btn.subtle { color: var(--text-muted); }
  .icon-btn.subtle:hover { color: var(--text-primary); }
  .icon-btn.primary {
    background: var(--accent-subtle);
    color: var(--accent);
  }
  .icon-btn.primary:hover { background: var(--accent); color: var(--text-on-accent); }
  .icon-btn.danger:hover { background: var(--error-subtle); color: var(--error); }

  .btn-ghost, .btn-primary {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 4px 10px;
    border-radius: var(--radius-sm);
    font-size: 11.5px;
    font-weight: 500;
    cursor: pointer;
    border: 1px solid transparent;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .btn-ghost {
    background: transparent;
    color: var(--text-secondary);
    border-color: var(--border-subtle);
  }
  .btn-ghost:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--border);
  }
  .btn-ghost:disabled { opacity: 0.4; pointer-events: none; }
  .btn-primary {
    background: var(--accent);
    color: var(--text-on-accent);
  }
  .btn-primary:hover { background: var(--accent-hover); }
  .btn-primary:disabled { opacity: 0.5; pointer-events: none; }
</style>
