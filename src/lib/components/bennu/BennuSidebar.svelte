<script lang="ts">
  /**
   * BennuSidebar — the Project tool window (the Java project file tree).
   *
   * Fed by `bennu_project_tree` via the project store. Uses the shared `Tree` widget
   * with **controlled expansion** (the UI store owns the expanded-id set) so the
   * header toolbar can Collapse-all / Expand-all and Select-opened-file can reveal a
   * path. Clicking a file opens it in the editor.
   *
   * Header actions (keyboard-reachable, all `.ps-btn`): New file (＋), Select opened
   * file (locate), Collapse all / Expand all (chevrons), and an Options kebab. New
   * file + a couple of Options entries are stubs (toast) until bennu-be serves them.
   *
   * Imports only shared/ui + shared FileExplorerModal + bennu-local store.
   */
  import {
    FolderOpen, Folder, FileCode2, FolderTree, Plus, Crosshair,
    ChevronsDownUp, ChevronsUpDown, MoreVertical,
    Copy, LocateFixed, ChevronDown, ChevronRight, FileText, FlaskConical, FileType2,
    History, Tag, Trash2, ExternalLink,
  } from 'lucide-svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { tick } from 'svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import Tree from '$lib/components/shared/ui/Tree.svelte';
  import type { RowSnippetCtx } from '$lib/components/shared/ui/Tree.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import BennuFilterBar from './BennuFilterBar.svelte';
  import BennuNewFileModal from './BennuNewFileModal.svelte';
  import BennuRenameFileModal from './BennuRenameFileModal.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { openFolder, revealFile } from '$lib/utils/reveal';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuIndexStore } from '$lib/stores/bennu/index.svelte';
  import { bennuTestStore } from '$lib/stores/bennu/tests.svelte';
  import { bennuCargoTestStore } from '$lib/stores/bennu/cargo-tests.svelte';
  import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
  import { bennuHistoryStore } from '$lib/stores/bennu/history.svelte';
  import { bennuFileOpsStore } from '$lib/stores/bennu/file-ops.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import type { DiscoveredRustTest, DiscoveredTest, TreeNode } from '$lib/types/bennu';
  import type { NewFileKind } from '$lib/ipc/bennu/scaffold';
  import { packageTree, isInPackageRoot } from './package-tree';
  // The shared file-icon vocabulary — the same one Corvus's tree and Sitta's explorer draw
  // from, so a `pom.xml` looks like a `pom.xml` wherever you meet it.
  import IconifyIconView from '@iconify/svelte';
  import type { IconifyIcon } from '@iconify/svelte';
  import { getFileIcon } from '$lib/utils/file-icons';
  import SymbolKindIcon from './SymbolKindIcon.svelte';
  import { javaKindStore } from '$lib/stores/bennu/java-kinds.svelte';

  /** Row height for the tree, in px.
   *
   *  A number and not a variable because the tree is **virtualized**: it multiplies this by
   *  the row index to place rows and to decide which slice to render, so it has to be
   *  arithmetic, not CSS. It is the one measurement here that cannot follow `--font-scale`,
   *  which is why it is set once, named, and left with room to spare — a row shorter than
   *  its content does not wrap, it clips. */
  const TREE_ROW_H = 26;

  let pickerOpen = $state(false);
  let filter = $state('');
  // The Tree instance — for its imperative `scrollToId` (the tree is virtualized, so a
  // DOM `scrollIntoView` on the selected row can't work when it's off-screen).
  let treeRef = $state<{ scrollToId: (id: string, block?: 'center' | 'nearest') => void } | null>(null);

  // The store's tree root is a single dir node; render its children as the top level
  // so the project folder itself isn't an extra nesting level.
  //
  // Under a source root the package chain is collapsed into one dotted row
  // (`it.acme.portal`), IntelliJ-style — three levels of indentation that say one name
  // are three levels not spent on the thing you were looking for. Paths are untouched,
  // so expansion, selection and opening all still key off exactly what they did.
  const rootChildren = $derived<TreeNode[]>(packageTree(projectStore.tree?.children ?? []));

  // ── File-tree icons (IntelliJ-style) ─────────────────────────────────────────
  // Per-`.java`-file type kind (class/interface/enum/record/annotation), from the project's class
  // index. Fetched into a plain map (the class index is async + cached) and refreshed when the index
  // rebuilds. A file that declares several types keys off the one matching the file name (the public
  // top-level type).
  $effect(() => {
    const root = projectStore.project?.root;
    // Reload when the index settles, NOT on `buildRevision` — that ticks once per file
    // walked, and a read per tick is how the backend was once talked into answering nothing.
    const busy = bennuIndexStore.indexing;
    // A Cargo project builds no class index (there are no classes) — skip the round-trip.
    if (!root || projectStore.isCargo) { javaKindStore.reset(); return; }
    if (busy) return;
    void javaKindStore.load(root, true);
  });


  /** The folder-icon tint by source-root role, so the tree conveys context at a glance.
   *
   *  Maven: main (blue) / test (green) / resources (amber) / webapp (purple). Cargo has its
   *  own conventional roots — `crates` and `src` are code (blue), `tests` / `benches` are
   *  test (green), `examples` and `content` are data/samples (amber) — and the Maven
   *  patterns are more specific, so they're tried first and a polyglot repo still reads
   *  correctly. */
  function folderColor(path: string): string {
    const p = path.replace(/\\/g, '/');
    if (/\/src\/(main|test)\/resources(\/|$)/.test(p)) return 'var(--warning)';
    if (/\/src\/test(\/|$)/.test(p)) return 'var(--success)';
    if (/\/src\/main\/webapp(\/|$)/.test(p)) return 'var(--color-tag, #c792ea)';
    if (/\/src\/main(\/|$)/.test(p)) return 'var(--info)';
    if (/\/(tests|benches)(\/|$)/.test(p)) return 'var(--success)';
    if (/\/(examples|content)(\/|$)/.test(p)) return 'var(--warning)';
    if (/\/(crates|src)(\/|$)/.test(p)) return 'var(--info)';
    return 'var(--text-muted)';
  }

  /** What a row draws: a lucide glyph this file tints itself, or one of the shared VS Code
   *  file icons (which carry their own colour and must not be tinted). */
  type RowIcon =
    | { shape: 'glyph'; icon: typeof FileCode2; color: string; filled?: boolean }
    | { shape: 'file'; icon: IconifyIcon }
    | { shape: 'kind'; kind: string };

  /** The icon for a tree node.
   *
   *  Two sources, because they answer different questions. A `.java` file gets the **kind it
   *  declares** — class, interface, enum, record, annotation — off the project's class index,
   *  which is what IntelliJ shows and what no by-extension table can know. Everything else
   *  goes to the shared resolver (`utils/file-icons`), the same one Corvus's tree and Sitta's
   *  explorer use, so `pom.xml`, `.gitlab-ci.yml`, `Dockerfile` and the rest look the same
   *  wherever you meet them — and an addition there shows up in all three at once.
   *
   *  This panel used to carry its own by-extension table of lucide glyphs. It was a second
   *  vocabulary for the same job, always the poorer of the two, and drifting. */
  /** The manifests that make a directory a **build unit** rather than a container of files.
   *
   *  One list, both ecosystems: a Cargo package and a Maven module are the same idea wearing
   *  two names — the folder where a build target is declared. */
  const MANIFESTS = ['Cargo.toml', 'pom.xml'];

  /** Whether this directory *defines* something the build knows about, i.e. holds a manifest.
   *
   *  Answered from the children the tree already carries, so it costs a scan of one level and
   *  no round-trip. A not-yet-expanded directory answers `false` — the tree is materialised
   *  in one shot today, and the day it isn't, an unexpanded folder is drawn as a folder,
   *  which is the honest default rather than a wrong claim. */
  function isBuildUnit(node: TreeNode): boolean {
    return node.children.some((c) => !c.is_dir && MANIFESTS.includes(c.name));
  }

  function iconFor(node: TreeNode): RowIcon {
    if (node.is_dir) {
      // A crate / Maven module root is where a build target is DECLARED, and everything
      // under it belongs to that target — a different kind of thing from a folder that
      // merely holds files. It stays a FOLDER glyph, because that is what it is; what
      // changes is the weight: filled and accented against everyone else's outline. A
      // second shape would have been a second vocabulary to learn for a distinction the
      // eye can read as emphasis.
      if (isBuildUnit(node)) {
        return { shape: 'glyph', icon: Folder, color: 'var(--accent)', filled: true };
      }
      // A directory inside a source root is a package, and reads as one — the row says
      // `it.acme.portal`, so a folder icon next to it would be describing the storage
      // rather than the thing. The source root itself keeps its folder icon: it is the
      // container the packages live in, not a package.
      // A package IS a folder — it is a directory holding files, and the dotted name on the
      // row already says it is a package. A box glyph next to it named a different kind of
      // thing than the one being pointed at. The tint carries the role (main / test /
      // resources / webapp), which is the distinction that actually helps.
      return { shape: 'glyph', icon: Folder, color: folderColor(node.path) };
    }
    const path = node.path.replace(/\\/g, '/');
    if (path.endsWith('.java')) {
      // The kind the file declares, as a lettered disc. A `.java` whose kind isn't indexed
      // yet (the index is still building) reads as a class — the overwhelmingly common
      // answer, and it settles the moment the index does.
      return { shape: 'kind', kind: javaKindStore.kindOf(path) };
    }
    return { shape: 'file', icon: getFileIcon(node.name) };
  }

  async function openProject(dir: string) {
    pickerOpen = false;
    try { await projectStore.openProject(dir); }
    catch { /* mock fallback already applied by the store */ }
  }

  function onRowSelect(node: TreeNode) {
    if (!node.is_dir) void projectStore.openFile(node.path);
  }

  // ── Controlled expansion (UI store owns the set) ─────────────────────────────
  function onExpandToggle(id: string, next: boolean) {
    bennuUiStore.setExpanded(id, next);
  }

  /** Every directory path in the (sub)tree — used by Expand-all. */
  function allDirIds(nodes: TreeNode[]): string[] {
    const out: string[] = [];
    const walk = (n: TreeNode) => {
      if (n.is_dir) { out.push(n.path); for (const c of n.children) walk(c); }
    };
    for (const n of nodes) walk(n);
    return out;
  }

  function collapseAll() { bennuUiStore.collapseAllTree(); }
  function expandAll() { bennuUiStore.expandTreeIds(allDirIds(rootChildren)); }

  /** Ancestor directory paths of a file path within the project tree. */
  function ancestorsOf(filePath: string): string[] {
    const out: string[] = [];
    const walk = (n: TreeNode, trail: string[]): boolean => {
      if (n.path === filePath) return true;
      if (!n.is_dir) return false;
      for (const c of n.children) {
        if (walk(c, [...trail, n.path])) { out.push(...trail, n.path); return true; }
      }
      return false;
    };
    for (const n of rootChildren) walk(n, []);
    return out;
  }

  /** Select-opened-file: expand the active file's ancestor folders, then scroll its
   *  row into view. `await tick()` lets the newly-expanded rows land in the Tree's
   *  flat list before `scrollToId` computes the row index. */
  async function revealActive() {
    const path = projectStore.activeFilePath;
    if (!path) return;
    bennuUiStore.expandTreeIds(ancestorsOf(path));
    await tick();
    treeRef?.scrollToId(path);
  }

  /**
   * Show the node in a file manager: a folder as the listing, a file selected inside its
   * folder — the same distinction every OS file manager makes, so "reveal" on a directory
   * does not open its parent with the folder highlighted.
   *
   * Routed through `$lib/utils/reveal` rather than calling the shell directly, because that
   * is where the OS-vs-built-in choice (Settings → File Explorer → "Open in the built-in
   * explorer") lives. Bypassing it would make this one entry the only place in the app that
   * ignores the setting.
   */
  async function revealInFileExplorer(node: TreeNode) {
    try {
      await (node.is_dir ? openFolder(node.path) : revealFile(node.path));
    } catch (e) {
      toastStore.show(`Could not reveal ${relativePath(node.path)}: ${e}`, 'error');
    }
  }

  // The toolbar's Select-opened-file + the palette both bump this relay.
  let lastReveal = 0;
  $effect(() => {
    const n = bennuUiStore.revealNonce;
    if (n !== lastReveal) { lastReveal = n; void revealActive(); }
  });

  /** Reveal a file in the tree: open it (so it becomes the selected row), expand its
   *  ancestor folders, and scroll the selected row into view. */
  async function revealPath(path: string) {
    await projectStore.openFile(path);
    bennuUiStore.expandTreeIds(ancestorsOf(path));
    await tick();
    treeRef?.scrollToId(path);
  }

  // ── Right-click context menu (read-only; FS mutations are a later wave) ──────
  /** `path` relative to the project root, forward slashes. Falls back to the
   *  absolute path when it isn't under the root. */
  function relativePath(path: string): string {
    const root = projectStore.project?.root;
    const fwd = path.replace(/\\/g, '/');
    if (!root) return fwd;
    const rootFwd = root.replace(/\\/g, '/').replace(/\/+$/, '');
    if (fwd === rootFwd) return '.';
    const prefix = rootFwd + '/';
    return fwd.startsWith(prefix) ? fwd.slice(prefix.length) : fwd;
  }

  function copyText(text: string) {
    void navigator.clipboard?.writeText(text).catch(() => { /* clipboard denied — ignore */ });
  }

  /** What "New ›" offers. Two entries for now — a Java type, and a plain file — because
   *  those are the two the tree is actually used to create; the dialog behind the first one
   *  is where the choice between class / interface / enum / … is made, exactly where
   *  IntelliJ puts it. Adding a third here is one line plus a case below. */
  const NEW_SUBMENU: MenuItem[] = $derived(
    projectStore.isCargo
      ? [
          { id: 'new-class', label: 'Rust File', icon: FileCode2 },
          { id: 'new-file',  label: 'File',      icon: FileText },
        ]
      : [
          { id: 'new-class', label: 'Java Class', icon: FileCode2 },
          { id: 'new-file',  label: 'File',       icon: FileText },
        ],
  );

  /** The Local History submenu.
   *
   *  The same three entries in the same order wherever it appears — the tree, the editor's
   *  tab — so where they are is learned once. `Show Deleted Files` lives in here rather
   *  than somewhere of its own because the question "where did that file go" is asked while
   *  looking at the folder it went from. */
  const HISTORY_SUBMENU: MenuItem[] = [
    { id: 'hist-show',    label: 'Show History',        icon: History, shortcut: 'Alt+Shift+H' },
    { id: 'hist-label',   label: 'Put Label…',          icon: Tag },
    { separator: true, id: 'sep-hist', label: '' },
    { id: 'hist-deleted', label: 'Show Deleted Files…', icon: Trash2 },
  ];

  /** Open the history at whatever the row stands for — a directory opens the folder scope,
   *  a file its own. */
  function showHistory(node: TreeNode) {
    const root = projectStore.project?.root;
    if (!root) return;
    if (node.is_dir) bennuHistoryStore.showFolder(root, node.path);
    else bennuHistoryStore.show(root, node.path);
  }

  /**
   * The test classes a tree node stands for: everything under a directory, or the ones a
   * file declares. Abstract bases are excluded — Surefire instantiates concrete classes, and
   * naming one only makes the run report that it matched nothing.
   */
  function testsFor(node: TreeNode) {
    if (projectStore.isCargo) {
      // No abstract-base equivalent in Rust: every `#[test]` is runnable where it is written.
      return node.is_dir
        ? bennuCargoTestStore.testsUnder(node.path)
        : bennuCargoTestStore.testsInFile(node.path);
    }
    return node.is_dir
      ? bennuTestStore.classesUnder(node.path)
      : bennuTestStore.classesInFile(node.path).filter((c) => !c.is_abstract);
  }

  /**
   * Run every test the node stands for.
   *
   * Two runners, two ways of naming a selection: Maven takes class names, cargo takes case refs
   * (package + target + path), and neither can express the other's. Which is why the branch is
   * here rather than behind a shared verb — a folder is not a scope either build system has.
   */
  function runTestsFor(node: TreeNode) {
    const root = projectStore.project?.root;
    if (!root) return;
    const found = testsFor(node);
    if (!found.length) return;
    if (projectStore.isCargo) {
      void bennuCargoTestStore.runCases(
        root,
        (found as DiscoveredRustTest[]).map((t) => bennuCargoTestStore.caseRefOf(t)),
      );
      return;
    }
    void bennuTestStore.runClasses(root, (found as DiscoveredTest[]).map((c) => c.selector));
  }

  function onRowContextMenu(node: TreeNode, e: MouseEvent) {
    openRowMenu(node, e.clientX, e.clientY);
  }

  /**
   * The row menu, opened at a point rather than at an event — because the keyboard opens it
   * too, and a `Shift+F10` has no cursor to sit under.
   */
  function openRowMenu(node: TreeNode, x: number, y: number) {
    // "New file…" creates in this directory (dir node) or the file's directory (file node).
    const newDir = node.is_dir ? node.path : parentDir(node.path);
    // Offered only where there is something to run — an entry that can only report "matched
    // nothing" is worse than no entry, and on a source tree most folders have no tests.
    const testCount = testsFor(node).length;
    const runItem: MenuItem[] = testCount
      ? [
          {
            id: 'run-tests',
            label: testCount === 1 ? 'Run test' : `Run ${testCount} tests`,
            icon: FlaskConical,
          },
          { separator: true, id: 'sep-tests', label: '' },
        ]
      : [];
    const items: MenuItem[] = node.is_dir
      ? [
          { id: 'new', label: 'New', icon: Plus, children: NEW_SUBMENU },
          { separator: true, id: 'sep-new', label: '' },
          ...runItem,
          { id: 'delete', label: 'Delete…', icon: Trash2, shortcut: 'Del', danger: true },
          { separator: true, id: 'sep-del-dir', label: '' },
          { id: 'history', label: 'Local History', icon: History, children: HISTORY_SUBMENU },
          { separator: true, id: 'sep-hist-dir', label: '' },
          { id: 'copy-path',     label: 'Copy path',          icon: Copy },
          { id: 'copy-rel',      label: 'Copy relative path', icon: Copy },
          { id: 'reveal-fs',     label: 'Reveal in File Explorer', icon: ExternalLink },
          { separator: true, id: 'sep-dir', label: '' },
          bennuUiStore.isExpanded(node.path)
            ? { id: 'collapse', label: 'Collapse', icon: ChevronRight }
            : { id: 'expand',   label: 'Expand',   icon: ChevronDown },
        ]
      : [
          { id: 'open',          label: 'Open',               icon: FolderOpen },
          { id: 'new', label: 'New', icon: Plus, children: NEW_SUBMENU },
          { separator: true, id: 'sep-file', label: '' },
          // Files only. Renaming a DIRECTORY is a different operation — for Rust it moves a whole
          // module path, and `willRenameFiles` would have to be asked per file inside it — so it is
          // absent rather than offered and then half-done.
          { id: 'rename',        label: 'Rename…',            icon: FileType2, shortcut: 'F2' },
          { id: 'delete',        label: 'Delete…',            icon: Trash2, shortcut: 'Del', danger: true },
          { separator: true, id: 'sep-rename', label: '' },
          ...runItem,
          { id: 'history', label: 'Local History', icon: History, children: HISTORY_SUBMENU },
          { separator: true, id: 'sep-hist-file', label: '' },
          { id: 'copy-path',     label: 'Copy path',          icon: Copy },
          { id: 'copy-rel',      label: 'Copy relative path', icon: Copy },
          { id: 'reveal',        label: 'Reveal in Project',  icon: LocateFixed },
          { id: 'reveal-fs',     label: 'Reveal in File Explorer', icon: ExternalLink },
        ];
    bennuContextMenuStore.show(x, y, items, (id) => {
      switch (id) {
        // The submenu leaf decides which shape the dialog opens in — a Java type, with its
        // kind list, or a plain file that only wants a name.
        case 'new-class': newFileKind = defaultKind(); newFileDir = newDir; break;
        case 'new-file':  newFileKind = 'file';  newFileDir = newDir; break;
        case 'run-tests': runTestsFor(node); break;
        case 'rename':    renamePath = node.path; break;
        case 'delete':    deleting = node; break;
        case 'open':      void projectStore.openFile(node.path); break;
        case 'copy-path': copyText(node.path); break;
        case 'copy-rel':  copyText(relativePath(node.path)); break;
        case 'reveal':    void revealPath(node.path); break;
        case 'reveal-fs': void revealInFileExplorer(node); break;
        case 'hist-show':  showHistory(node); break;
        // Labelling needs a revision to pin the name on, and the dialog is where one is
        // chosen — so this opens it rather than inventing a second way to pick.
        case 'hist-label': showHistory(node); break;
        case 'hist-deleted':
          if (projectStore.project) bennuHistoryStore.showDeleted(projectStore.project.root);
          break;
        case 'expand':    bennuUiStore.setExpanded(node.path, true); break;
        case 'collapse':  bennuUiStore.setExpanded(node.path, false); break;
      }
    });
  }

  // ── Delete ──────────────────────────────────────────────────────────────────
  /** The node a delete confirmation is open for (null = closed). */
  let deleting = $state<TreeNode | null>(null);

  /** What the confirmation says will happen.
   *
   *  A directory is spelled out as "and everything in it" rather than counted: counting
   *  means walking the tree to produce a number that is stale by the time it is read, and
   *  the sentence that matters is the one about the folder, not the arithmetic. */
  const deleteDetail = $derived.by(() => {
    const node = deleting;
    if (!node) return '';
    const what = node.is_dir
      ? `${relativePath(node.path)} and everything in it`
      : relativePath(node.path);
    return `${what}\n\nIt goes into Bennu's local history, not the system trash — undo it right away with Ctrl/Cmd+Z, or restore it later from Local History › Deleted.`;
  });

  async function confirmDelete() {
    const node = deleting;
    const root = projectStore.project?.root;
    deleting = null;
    if (!node || !root) return;
    await bennuFileOpsStore.delete(root, [node.path]);
  }

  /**
   * The tree's own undo.
   *
   * Bound here and not at the window level on purpose: <kbd>⌘Z</kbd> in the editor means
   * "un-type that" and here it means "un-delete that", and a single handler for both is
   * how you press it expecting a keystroke back and get a file instead. Whoever has focus
   * answers.
   */
  function onTreeKeydown(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (!mod || e.shiftKey || e.altKey || e.key.toLowerCase() !== 'z') return;
    if (!bennuFileOpsStore.undoable) return;
    e.preventDefault();
    e.stopPropagation();
    void bennuFileOpsStore.undo();
  }

  /** Parent directory of a path (forward-slash aware). */
  function parentDir(p: string): string {
    return p.replace(/[\\/][^\\/]*$/, '') || p;
  }

  /** Where the header / kebab "New file…" creates: the active file's directory, else the root. */
  function defaultNewDir(): string {
    const active = projectStore.activeFilePath;
    return active ? parentDir(active) : (projectStore.project?.root ?? '');
  }

  /**
   * The keys a focused row owns: Delete, F2 to rename, and Shift+F10 / Menu for its context menu.
   *
   * Here rather than in the window's key handler because they are about the row that has focus,
   * which only the tree knows. Directories are skipped for F2 for the same reason the menu entry
   * is files-only.
   */
  function onRowKeydown(node: TreeNode, e: KeyboardEvent) {
    // Delete on the focused row — the key every file manager binds, and the one the
    // context menu advertises. Backspace too, because that is what deletes on a Mac.
    if (e.key === 'Delete' || e.key === 'Backspace') {
      e.preventDefault();
      deleting = node;
      return;
    }
    // The keyboard route to the row menu — `Shift+F10` on every platform, plus the dedicated
    // Menu key where the keyboard has one. Without it every entry below Delete and Rename is
    // mouse-only, which for a tree that is meant to be driven from the keyboard is a hole.
    // Anchored to the row, so the menu opens where the focus is instead of where the pointer
    // was last left.
    if (e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey)) {
      e.preventDefault();
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      openRowMenu(node, r.left + 16, r.bottom);
      return;
    }
    if (e.key !== 'F2' || node.is_dir) return;
    e.preventDefault();
    renamePath = node.path;
  }

  /** The file a Rename dialog is open for (null = closed). */
  let renamePath = $state<string | null>(null);

  // The directory a New-file modal is open for (null = closed).
  let newFileDir = $state<string | null>(null);
  /** Which shape the New dialog opens in — set by the submenu leaf that opened it. The
   *  header's `＋` leaves it at the typed default, the overwhelmingly common thing to create
   *  here; which template that is follows the project (a struct on Cargo, a class on Maven). */
  let newFileKind = $state<NewFileKind>('class');

  /** The typed default for this project. Read at the moment the dialog is opened rather
   *  than held in state — the project can change under a long-lived panel. */
  function defaultKind(): NewFileKind {
    return projectStore.isCargo ? 'rust_struct' : 'class';
  }

  // ── Options kebab ────────────────────────────────────────────────────────────
  const optionsMenu: DropdownItem[] = [
    { kind: 'item', id: 'expand',   label: 'Expand all',   icon: ChevronsUpDown,   onclick: expandAll },
    { kind: 'item', id: 'collapse', label: 'Collapse all', icon: ChevronsDownUp,   onclick: collapseAll },
    { kind: 'separator' },
    { kind: 'item', id: 'reveal',   label: 'Select opened file', icon: Crosshair, onclick: revealActive },
    { kind: 'separator' },
    { kind: 'item', id: 'newfile',  label: 'New file…', icon: Plus,
      onclick: () => { newFileKind = defaultKind(); newFileDir = defaultNewDir(); } },
  ];
</script>

<PanelShell title="Project">
  {#snippet icon()}<FolderTree size={13} />{/snippet}
  {#snippet actions()}
    <button class="ps-btn" type="button" onclick={() => { newFileKind = defaultKind(); newFileDir = defaultNewDir(); }} disabled={!projectStore.project} use:tooltip={'New file'} aria-label="New file">
      <Plus size={14} />
    </button>
    <button class="ps-btn" type="button" onclick={revealActive} disabled={!projectStore.activeFilePath} use:tooltip={'Select opened file'} aria-label="Select opened file">
      <Crosshair size={14} />
    </button>
    <button class="ps-btn" type="button" onclick={collapseAll} use:tooltip={'Collapse all'} aria-label="Collapse all">
      <ChevronsDownUp size={14} />
    </button>
    <button class="ps-btn" type="button" onclick={expandAll} use:tooltip={'Expand all'} aria-label="Expand all">
      <ChevronsUpDown size={14} />
    </button>
    <Dropdown items={optionsMenu} position="fixed" direction="down" width="200px">
      {#snippet trigger({ open, toggle })}
        <button class="ps-btn" class:ps-btn-active={open} type="button" onclick={toggle} use:tooltip={'Options'} aria-label="Project options" aria-haspopup="menu" aria-expanded={open}>
          <MoreVertical size={14} />
        </button>
      {/snippet}
    </Dropdown>
  {/snippet}
  {#if projectStore.project}
    {#snippet toolbar()}
      <BennuFilterBar bind:query={filter} placeholder="Filter files…" />
    {/snippet}
  {/if}

  {#if projectStore.project}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="bs-tree" onkeydown={onTreeKeydown} role="presentation">
      <Tree
        bind:this={treeRef}
        nodes={rootChildren}
        rowHeight={TREE_ROW_H}
        getId={(n) => n.path}
        getChildren={(n) => (n.is_dir ? n.children : undefined)}
        selectedId={projectStore.activeFilePath}
        expandedIds={bennuUiStore.treeExpanded}
        {onExpandToggle}
        {filter}
        ariaLabel="Project files"
        onSelect={onRowSelect}
        onContextMenu={onRowContextMenu}
        onRowKeydown={onRowKeydown}
      >
        {#snippet row(ctx: RowSnippetCtx<TreeNode>)}
          {@const meta = iconFor(ctx.node)}
          <!-- Two ways of being present-but-not-part-of-the-project, and they stack: a
               gitignored entry reads faintest, a merely hidden one sits between it and a
               normal row. Both stay legible — this is a de-emphasis, not a strikethrough. -->
          {@const faint = ctx.node.ignored ? 'faint-ignored' : ctx.node.hidden ? 'faint-hidden' : ''}
          {#if meta.shape === 'glyph'}
            {@const Glyph = meta.icon}
            <span class="tree-icon {faint}" style="color: {meta.color}">
              <Glyph size={14} fill={meta.filled ? 'currentColor' : 'none'} />
            </span>
          {:else if meta.shape === 'kind'}
            <span class="tree-icon {faint}"><SymbolKindIcon kind={meta.kind} /></span>
          {:else}
            <!-- A file-type icon carries its own colours (it IS the brand mark), so this one
                 is not tinted — a `color` here would only fight it. -->
            <span class="tree-icon {faint}">
              <IconifyIconView icon={meta.icon} width={15} height={15} />
            </span>
          {/if}
          <span
            class="tree-label {faint}"
            use:tooltip={ctx.node.ignored ? 'Ignored by git' : ''}
          >{ctx.node.name}</span>
        {/snippet}
      </Tree>
    </div>
  {:else}
    <div class="bs-empty">
      <EmptyState message="No project open." />
      <button class="bs-empty-action" type="button" onclick={() => (pickerOpen = true)}>
        <FolderOpen size={14} /> Open project…
      </button>
    </div>
  {/if}
</PanelShell>

{#if pickerOpen}
  <FileExplorerModal
    mode="folder"
    title="Open project (Maven or Cargo)"
    onConfirm={openProject}
    onCancel={() => (pickerOpen = false)}
    onClose={() => (pickerOpen = false)}
  />
{/if}

{#if newFileDir !== null}
  <BennuNewFileModal
    dir={newFileDir}
    initialKind={newFileKind}
    onClose={() => (newFileDir = null)}
  />
{/if}

{#if renamePath !== null}
  <BennuRenameFileModal path={renamePath} onClose={() => (renamePath = null)} />
{/if}

{#if deleting}
  <ConfirmModal
    title={deleting.is_dir ? 'Delete this folder?' : 'Delete this file?'}
    message={deleting.name}
    detail={deleteDetail}
    variant="danger"
    confirmLabel="Delete"
    busy={bennuFileOpsStore.busy}
    onConfirm={() => void confirmDelete()}
    onCancel={() => (deleting = null)}
  />
{/if}

<style>
  .bs-tree {
    flex: 1; min-height: 0;
    overflow-y: auto;
    padding: 2px 0 6px;
  }

  .bs-empty {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 10px; min-height: 0; padding: 12px;
  }
  .bs-empty-action {
    display: inline-flex; align-items: center; gap: 6px;
    height: 28px; padding: 0 12px;
    background: var(--bg-input); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md); color: var(--text-secondary);
    font-size: var(--font-size-sm); cursor: pointer;
    transition: border-color var(--transition-fast), color var(--transition-fast);
  }
  .bs-empty-action:hover { border-color: var(--border-focus, var(--accent)); color: var(--text-primary); }

  /* The project tree reads a step larger than the app's default list size — it is the panel
     you scan continuously, and at 12px a package row is a lot of small text.

     The icon is sized from the SAME variable rather than from its own number, so the two
     move together: `--font-scale` (the Appearance setting) then scales the whole row, icons
     included, instead of growing the text around fixed glyphs. The `size` / `width` props on
     the components are floors the rule below overrides — one rule for both, since lucide and
     Iconify each render a plain `<svg>`. */
  .bs-tree :global(.tree-row) { font-size: var(--font-size-md); }
  .tree-icon {
    display: flex; align-items: center; color: var(--text-muted);
    font-size: calc(var(--font-size-md) * 1.25);
  }
  .tree-icon :global(svg) { width: 1em; height: 1em; }
  .tree-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  /* Hidden and gitignored rows. Opacity rather than a colour so the icon's own palette (a
     `.gitignore`'s brand mark, a package's tint) fades with the text instead of being
     overridden by it — the row still says what kind of thing it is, quietly. */
  .faint-hidden  { opacity: 0.62; }
  .faint-ignored { opacity: 0.42; }
</style>
