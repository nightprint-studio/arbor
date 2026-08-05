<script lang="ts">
  /**
   * The Go-to navigator — classes, files and symbols in one box.
   *
   * It is the shared {@link NavigateTo} overlay (the same one Picus opens on
   * <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd>) with Bennu's three sources plugged into
   * it. Everything that makes the box a tool rather than a filter — the subsequence
   * scoring, the matched characters lit in the row, the `in:` / `ext:` / `sort:` directives,
   * the per-category grouping under **All**, the keyboard — lives there and is not written
   * twice. What used to be here was a second, weaker implementation of the same thing:
   * one mode at a time, substring-anchored, no highlighting, no directives.
   *
   * The rail of shortcuts still lands on the right tab — <kbd>Ctrl</kbd>+<kbd>N</kbd> on
   * Classes, <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>N</kbd> on Files — and
   * <kbd>Tab</kbd> moves between them without reopening.
   *
   * ## Where each source comes from
   *
   * - **Classes** — `bennu_class_index`, served from the index the background build
   *   already produced (instant after the first index; a fresh scan otherwise).
   * - **Files** — flattened from the project tree already in memory. Binaries are dropped:
   *   they cannot be opened, so offering them is offering a dead end.
   * - **Symbols** — the `members` index: every method and field the project declares, with
   *   its owner and signature as the detail. Entries with no navigable site are dropped for
   *   the same reason.
   * - **Library classes / Library files** — what is on the **dependency classpath** but
   *   nowhere in the tree. Present only when the setting asks for them, and unlike the three
   *   above they are *searched in the backend* rather than fetched: a legacy classpath is
   *   hundreds of thousands of entries, and handing that over to filter in the page would cost
   *   tens of megabytes to answer a question about twenty of them. The backend narrows, this
   *   side still ranks and highlights — see {@link NavigateTo}'s `search`.
   */
  import { Braces, FileArchive, Package } from 'lucide-svelte';
  import NavigateTo, {
    type NavigateCategory,
    type NavigateItem,
  } from '$lib/components/shared/navigate/NavigateTo.svelte';
  import type { IconComponent } from '$lib/types/icon';
  import JavaKindIconRaw from './JavaKindIcon.svelte';
  import BennuFileIconRaw from './BennuFileIcon.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuIndexStore } from '$lib/stores/bennu/index.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
  import { indexEntries } from '$lib/ipc/bennu/inspect';
  import { libraryClasses, libraryFiles, openLibraryFile } from '$lib/ipc/bennu/library';
  import { openLibraryClass } from './log-link';
  import type { TreeNode } from '$lib/types/bennu';

  let { onClose }: { onClose: () => void } = $props();

  // The row's `icon` slot is typed for lucide's class-based components; a Svelte 5 `.svelte`
  // component is a function, so it needs the same cast the palette's icon map uses.
  const JavaKindIcon = JavaKindIconRaw as unknown as IconComponent;
  const BennuFileIcon = BennuFileIconRaw as unknown as IconComponent;

  /** Which tab the shortcut that opened this asked for. */
  const initialCategory = $derived(
    { class: 'classes', file: 'files', symbol: 'symbols', all: 'all' }[bennuUiStore.navMode],
  );

  /** Open `file`, then jump — the editor must exist before it can be scrolled. */
  function go(file: string, line: number | null) {
    void projectStore.openFile(file).then(() => {
      if (line) bennuUiStore.requestGoto(line);
    });
  }

  // ── files ───────────────────────────────────────────────────────────────────
  const BINARY =
    /\.(png|jpe?g|gif|bmp|ico|webp|xcf|psd|pdf|zip|jar|war|ear|class|exe|dll|so|dylib|bin|o|obj|a|lib|7z|gz|tar|rar|mp3|mp4|wav|avi|mov|mkv|ttf|otf|woff2?|eot|db|sqlite)$/i;

  function flattenFiles(node: TreeNode | null, out: NavigateItem[]) {
    if (!node) return;
    if (!node.is_dir) {
      if (!BINARY.test(node.name)) {
        out.push({
          id: node.path,
          name: node.name,
          detail: dirOf(node.path),
          // The tree's own icon rule, not a stand-in for it — a `.java` wears the kind it
          // declares, everything else its file type.
          icon: BennuFileIcon,
          iconProps: { path: node.path },
          onOpen: () => go(node.path, null),
        });
      }
      return;
    }
    for (const c of node.children) flattenFiles(c, out);
  }

  /** The directory part of a path, project-relative when we can tell. */
  function dirOf(path: string): string {
    const root = projectStore.project?.root?.replace(/[\\/]+$/, '') ?? '';
    const norm = path.replace(/\\/g, '/');
    const rel = root && norm.startsWith(root.replace(/\\/g, '/'))
      ? norm.slice(root.length + 1)
      : norm;
    const cut = rel.lastIndexOf('/');
    return cut < 0 ? '' : rel.slice(0, cut);
  }

  // ── classes ─────────────────────────────────────────────────────────────────

  /** The package part of a FQCN — what tells four same-named classes apart. */
  function packageOf(fqcn: string): string {
    const cut = fqcn.lastIndexOf('.');
    return cut < 0 ? '' : fqcn.slice(0, cut);
  }

  /**
   * The two backend-searched categories. Separate from the list above because they are a
   * different kind of source — `search` instead of `items` — and because reading them in one
   * place is what makes it obvious that only these two leave the page per keystroke.
   */
  const libraryCategories = $derived<NavigateCategory[]>([
    {
      id: 'lib-classes',
      label: 'Library classes',
      emptyMessage: 'Type to search the dependency classpath.',
      search: async (text) => {
        const root = projectStore.project?.root;
        if (!root) return [];
        const found = await libraryClasses(root, text);
        return found.map((c) => ({
          id: `${c.fqcn}@${c.jar}`,
          name: c.simple,
          detail: c.package,
          icon: Package,
          // The artifact, on the right — which of four versions of the same class you are
          // about to open is the question a classpath makes you ask.
          tag: c.jar,
          onOpen: () => void openLibraryClass(c.fqcn),
        }));
      },
    },
    {
      id: 'lib-files',
      label: 'Library files',
      emptyMessage: 'Type to search the files inside the dependency jars.',
      search: async (text) => {
        const root = projectStore.project?.root;
        if (!root) return [];
        const found = await libraryFiles(root, text);
        return found.map((f) => ({
          id: f.id,
          name: f.name,
          // The path INSIDE the jar: two `web.xml`s from two artifacts are told apart by it.
          detail: f.entry,
          icon: FileArchive,
          tag: f.jar,
          onOpen: () => void openJarEntry(f.id),
        }));
      },
    },
  ]);

  /** Extract a jar entry to the read-only cache and open it. A resource inside a jar has no
   *  path until something writes it out — that is what the backend call is for. */
  async function openJarEntry(id: string) {
    const root = projectStore.project?.root;
    if (!root) return;
    try {
      await projectStore.openFile(await openLibraryFile(root, id));
    } catch {
      /* a classpath that changed under a stale id — nothing worth a dialog over */
    }
  }

  const categories = $derived<NavigateCategory[]>([
    {
      id: 'classes',
      label: 'Classes',
      emptyMessage: 'No classes indexed yet.',
      items: async () => {
        const root = projectStore.project?.root;
        if (!root) return [];
        const classes = await bennuIndexStore.classesForRoot(root);
        return classes.map((c) => ({
          id: `${c.fqcn}@${c.file}:${c.line}`,
          name: c.simple,
          detail: packageOf(c.fqcn),
          // The lettered ring the tree and the editor tabs use — C / I / E / R / @ — rather
          // than a second vocabulary of lucide shapes for the same five kinds.
          icon: JavaKindIcon,
          iconProps: { kind: c.kind },
          tag: c.kind && c.kind !== 'class' ? c.kind : undefined,
          onOpen: () => go(c.file, c.line),
        }));
      },
    },
    {
      id: 'files',
      label: 'Files',
      emptyMessage: 'No files in this project.',
      items: () => {
        const out: NavigateItem[] = [];
        flattenFiles(projectStore.tree, out);
        return out;
      },
    },
    {
      id: 'symbols',
      label: 'Symbols',
      emptyMessage: 'No symbols indexed yet.',
      items: async () => {
        const root = projectStore.project?.root;
        if (!root) return [];
        const members = await indexEntries(root, 'members');
        // A member with no source site can be listed but not navigated to — drop it rather
        // than offer a row that does nothing.
        return members
          .filter((m) => !!m.file)
          .map((m, i) => ({
            id: `${m.primary}@${m.file}:${m.line ?? 0}:${i}`,
            name: m.primary,
            detail: m.secondary,
            icon: Braces,
            onOpen: () => go(m.file as string, m.line ?? null),
          }));
      },
    },
    // ── the classpath ─────────────────────────────────────────────────────────
    // Opt-in, and last: with them on, a two-letter query reaches a hundred thousand things
    // nobody in this project wrote, and what you were looking for is usually one of the three
    // above. Kept behind the setting so the default box stays a box about YOUR code.
    ...(bennuSettingsStore.searchDependencies ? libraryCategories : []),
  ]);

</script>

<NavigateTo
  {categories}
  {initialCategory}
  initialQuery={bennuUiStore.navInitial}
  title="Go to"
  {onClose}
/>
