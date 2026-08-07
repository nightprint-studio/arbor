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
   * - **The dependency classpath** — what the build depends on but nobody here wrote. Not a tab
   *   of its own: Classes and Files each declare three **sources** — Project, Project &
   *   dependencies, Dependencies — picked from one control on the header row, so looking for a
   *   `HttpServletRequest` is one list ranked together rather than two tabs checked in turn.
   *   Unlike the project's own, classpath rows are *searched in the backend* rather than
   *   fetched: a legacy classpath is hundreds of thousands of entries, and handing that over to
   *   filter in the page would cost tens of megabytes to answer a question about twenty of them.
   *   The backend narrows, this side still ranks and highlights. Those rows are **tinted** and
   *   name their **artifact**, because what you can edit and what you can only read are not the
   *   same kind of answer.
   */
  import { Braces, FolderTree, Package } from 'lucide-svelte';
  import ToggleButton from '$lib/components/shared/ui/ToggleButton.svelte';
  import NavigateTo, {
    type NavigateCategory,
    type NavigateItem,
    type NavigatePreview,
    type NavigateSource,
  } from '$lib/components/shared/navigate/NavigateTo.svelte';
  import { projectTree, readFile } from '$lib/ipc/bennu';
  import { frameSource } from '$lib/ipc/bennu/nav';
  import { languageForPath } from './languages';
  import { moduleIndex, relativeTo } from './modules';
  import { kindGlyph } from './symbol-kind-glyph';
  import type { IconComponent } from '$lib/types/icon';
  import SymbolKindIconRaw from './SymbolKindIcon.svelte';
  import BennuFileIconRaw from './BennuFileIcon.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuIndexStore } from '$lib/stores/bennu/index.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
  import { indexEntries } from '$lib/ipc/bennu/inspect';
  import { lspWorkspaceSymbols } from '$lib/ipc/bennu/lsp';
  import { libraryClasses, libraryFiles, openLibraryFile } from '$lib/ipc/bennu/library';
  import { openLibraryClass } from './log-link';
  import type { TreeNode } from '$lib/types/bennu';

  let { onClose }: { onClose: () => void } = $props();

  // The row's `icon` slot is typed for lucide's class-based components; a Svelte 5 `.svelte`
  // component is a function, so it needs the same cast the palette's icon map uses.
  const SymbolKindIcon = SymbolKindIconRaw as unknown as IconComponent;
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

  // ── modules ─────────────────────────────────────────────────────────────────
  /** The module lookup, rebuilt when the project changes — see {@link moduleIndex}, which Find
   *  in project narrows by too. */
  const modules = $derived(moduleIndex(projectStore.project));

  /** A project-relative, forward-slashed path. */
  function relOf(file: string): string { return modules.relative(file); }

  /** Which module a file belongs to, or `undefined` on a single-module project. */
  function moduleOf(file: string): string | undefined { return modules.moduleOf(file); }

  // ── preview ─────────────────────────────────────────────────────────────────
  /** Where each item points. Kept beside the items rather than encoded in their ids, which are
   *  for identity and would have to be parsed back apart to be useful. */
  const sites = new Map<string, { file: string; line: number | null }>();
  /** Files already read this opening, so walking the list re-reads nothing. */
  const fileCache = new Map<string, string>();

  /**
   * The selected entry's file, whole.
   *
   * Not a window around the declaration: the column is a real read-only editor, so handing it
   * the document gives real line numbers, correct multi-line constructs, and the ability to
   * scroll past what somebody guessed you wanted to see. CodeMirror renders only the viewport,
   * so a 10 000-line file costs what the visible dozen lines cost.
   */
  async function previewOf(item: NavigateItem): Promise<NavigatePreview | null> {
    const root = projectStore.project?.root;
    if (!root) return null;
    const site = sites.get(item.id);
    if (!site) return classpathPreview(item, root);
    let text = fileCache.get(site.file);
    if (text === undefined) {
      text = (await readFile(root, site.file)).text;
      fileCache.set(site.file, text);
    }
    return {
      title: relOf(site.file),
      text,
      language: languageForPath(site.file),
      activeLine: site.line,
    };
  }

  /**
   * What a **classpath** row needs before it can be previewed: a class has to have its source
   * view served (the real `.java` when a `-sources.jar` is on disk, else a decompiled stub), a
   * resource has to be extracted out of its zip.
   *
   * Kept apart from {@link sites} because these are a *call*, not a path — and the call is the
   * expensive one, which is why the overlay only makes it once the selection has settled and why
   * the answer is cached by the row's identity: walking back up the list re-reads nothing.
   */
  type ClasspathSite = { kind: 'class'; fqcn: string } | { kind: 'file'; id: string };
  const classpathSites = new Map<string, ClasspathSite>();
  const classpathCache = new Map<string, NavigatePreview>();

  /** Record where a classpath row points, and hand it back — the {@link at} of the other half. */
  function from(item: NavigateItem, site: ClasspathSite): NavigateItem {
    classpathSites.set(item.id, site);
    return item;
  }

  async function classpathPreview(
    item: NavigateItem,
    root: string,
  ): Promise<NavigatePreview | null> {
    const cached = classpathCache.get(item.id);
    if (cached) return cached;
    const site = classpathSites.get(item.id);
    if (!site) return null;
    try {
      // Both end in a file the editor could open — the same one opening the row would produce,
      // which is what keeps the preview and the tab from being two different readings.
      const view = site.kind === 'class' ? await frameSource(root, site.fqcn) : null;
      const path = site.kind === 'class' ? view?.file : await openLibraryFile(root, site.id);
      if (!path) return null;
      const text = (await readFile(root, path)).text;
      const answer: NavigatePreview = {
        title: site.kind === 'class' ? site.fqcn : site.id,
        text,
        language: languageForPath(path),
        // The backend decided where to land knowing something this side cannot: whether it
        // served real source (a line is a fact) or a stub (the numbers are fiction, so it points
        // at the declaration instead).
        activeLine: view?.offset ? lineOfOffset(text, view.offset) : null,
      };
      classpathCache.set(item.id, answer);
      return answer;
    } catch {
      return null;
    }
  }

  /** 1-based line holding `offset`. The source view answers in offsets; the preview scrolls by
   *  line. */
  function lineOfOffset(text: string, offset: number): number {
    let line = 1;
    for (let i = 0; i < offset && i < text.length; i += 1) if (text[i] === '\n') line += 1;
    return line;
  }

  /** Record where an item points, and hand it straight back — so a category's `items()` reads
   *  as one expression rather than as a build-then-register pair. */
  function at(item: NavigateItem, file: string, line: number | null): NavigateItem {
    sites.set(item.id, { file, line });
    return item;
  }

  // ── files ───────────────────────────────────────────────────────────────────
  const BINARY =
    /\.(png|jpe?g|gif|bmp|ico|webp|xcf|psd|pdf|zip|jar|war|ear|class|exe|dll|so|dylib|bin|o|obj|a|lib|7z|gz|tar|rar|mp3|mp4|wav|avi|mov|mkv|ttf|otf|woff2?|eot|db|sqlite)$/i;

  /** Flatten one project's tree into rows. `home` names the project for a workspace member and
   *  is undefined for the active one — see {@link searchRoots}. */
  function flattenFiles(node: TreeNode | null, out: NavigateItem[], root: string, home?: string) {
    if (!node) return;
    if (!node.is_dir) {
      if (!BINARY.test(node.name)) {
        const where = home ?? moduleOf(node.path);
        out.push(
          at(
            {
              id: node.path,
              name: node.name,
              detail: dirOf(node.path, root),
              // The tree's own icon rule, not a stand-in for it — a `.java` wears the kind it
              // declares, everything else its file type.
              icon: BennuFileIcon,
              iconProps: { path: node.path },
              facet: where,
              origin: where,
              onOpen: () => go(node.path, null),
            },
            node.path,
            null,
          ),
        );
      }
      return;
    }
    for (const c of node.children) flattenFiles(c, out, root, home);
  }

  /** The directory part of a path, relative to the project that holds it. */
  function dirOf(path: string, root: string): string {
    const rel = relativeTo(root, path);
    const cut = rel.lastIndexOf('/');
    return cut < 0 ? '' : rel.slice(0, cut);
  }

  // ── classes ─────────────────────────────────────────────────────────────────

  /** The package part of a FQCN — what tells four same-named classes apart. */
  function packageOf(fqcn: string): string {
    const cut = fqcn.lastIndexOf('.');
    return cut < 0 ? '' : fqcn.slice(0, cut);
  }

  // ── the four suppliers ──────────────────────────────────────────────────────
  //
  // Two of the project's own, pulled once per opening, and two of the classpath's, searched in
  // the backend per query. They are written as plain functions rather than as categories because
  // they are no longer one tab each: the sources below combine them.

  /** Also look in the other projects of the workspace. A toggle beside the field, the same one
   *  Find in project has and in the same place: it is one bit, and it re-runs the search. */
  let workspace = $state(false);

  /**
   * The projects one search covers: the active one, plus every other member of the workspace
   * when the toggle asks for them.
   *
   * `home` is what a row from that project says it is from — undefined for the active project,
   * whose rows are filed by *module* instead, because inside the project you are in, the module
   * is the distinction that matters and the project name is on every row.
   */
  const searchRoots = $derived.by<{ root: string; home?: string }[]>(() => {
    const active = projectStore.project?.root;
    if (!active) return [];
    const mine = [{ root: active }];
    if (!workspace || !projectStore.hasWorkspace) return mine;
    return [
      ...mine,
      ...projectStore.workspaceProjects
        .filter((p) => p.root !== active)
        .map((p) => ({ root: p.root, home: p.name })),
    ];
  });

  async function projectClasses(): Promise<NavigateItem[]> {
    const lists = await Promise.all(
      searchRoots.map(async ({ root, home }) => {
        // Per root, and cached per root by the store — a workspace member's index is built the
        // first time it is searched and never again.
        const classes = await bennuIndexStore.classesForRoot(root).catch(() => []);
        return classes.map((c) =>
          at(
            {
              id: `${c.fqcn}@${c.file}:${c.line}`,
              name: c.simple,
              detail: packageOf(c.fqcn),
              // The lettered ring the tree and the editor tabs use — C / I / E / R / @ — rather
              // than a second vocabulary of lucide shapes for the same five kinds.
              icon: SymbolKindIcon,
              iconProps: { kind: c.kind },
              tag: c.kind && c.kind !== 'class' ? c.kind : undefined,
              // A foreign project has its own module list, which this one has not read — so its
              // rows are filed under the project rather than under a module resolved against the
              // wrong tree, which is how a class ends up filed in a module it is not in.
              facet: home ?? moduleOf(c.file),
              origin: home ?? moduleOf(c.file),
              onOpen: () => go(c.file, c.line),
            },
            c.file,
            c.line,
          ),
        );
      }),
    );
    return lists.flat();
  }

  async function projectFiles(): Promise<NavigateItem[]> {
    const out: NavigateItem[] = [];
    for (const { root, home } of searchRoots) {
      // The active project's tree is already in memory; a workspace member's has to be asked
      // for, which is why this half only costs anything when the toggle is on.
      const tree = home === undefined
        ? projectStore.tree
        : await projectTree(root).catch(() => null);
      flattenFiles(tree, out, root, home);
    }
    return out;
  }

  async function classpathClasses(text: string): Promise<NavigateItem[]> {
    const root = projectStore.project?.root;
    if (!root) return [];
    const found = await libraryClasses(root, text);
    return found.map((c) =>
      from(
        {
          id: `${c.fqcn}@${c.jar}`,
          name: c.simple,
          detail: c.package,
          icon: Package,
          // The artifact — which of four versions of the same class you are about to open is the
          // question a classpath makes you ask, and it sits in the same slot a project class
          // puts its module in.
          origin: c.jar,
          external: true,
          onOpen: () => void openLibraryClass(c.fqcn),
        },
        { kind: 'class', fqcn: c.fqcn },
      ),
    );
  }

  async function classpathFiles(text: string): Promise<NavigateItem[]> {
    const root = projectStore.project?.root;
    if (!root) return [];
    const found = await libraryFiles(root, text);
    return found.map((f) =>
      from(
        {
          id: f.id,
          name: f.name,
          // The path INSIDE the jar: two `web.xml`s from two artifacts are told apart by it.
          detail: f.entry,
          // The tree's own icon rule, by the entry's own name: a `.xml` from a jar is an XML
          // file and reads like one once opened, so drawing every one of them as a generic
          // archive glyph answered "what kind of file is this" with a shrug — and it was the
          // one list left in Bennu still doing that.
          icon: BennuFileIcon,
          iconProps: { path: f.entry },
          origin: f.jar,
          external: true,
          onOpen: () => void openJarEntry(f.id),
        },
        { kind: 'file', id: f.id },
      ),
    );
  }

  /**
   * Where Classes and Files draw from — one picker for both, rather than a *Library classes* and
   * a *Library files* tab beside them.
   *
   * The tabs made the overlay's top level answer "where might it be" instead of "what am I
   * looking for", and left you checking two of them for one question. **Project & dependencies**
   * scores the two together into a single ranked list, which is the shape the question actually
   * has: you want the `HttpServletRequest`, and whether it is yours or the container's is the
   * answer, not the search.
   */
  function sourcesFor(
    project: NavigateSource['items'],
    classpath: NavigateSource['search'],
    what: string,
  ): NavigateSource[] {
    return [
      { id: 'project', label: 'Project', items: project },
      { id: 'both', label: 'Project & dependencies', items: project, search: classpath },
      {
        id: 'dependencies',
        label: 'Dependencies',
        search: classpath,
        emptyMessage: `Type to search the ${what} inside the dependency jars.`,
      },
    ];
  }

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

  // ── Types and symbols from a language server ────────────────────────────────────
  //
  // The Java categories above are backed by Bennu's own index, which a Cargo project does not
  // have — so <kbd>Ctrl</kbd>+<kbd>N</kbd> found nothing there. A language server answers the same
  // question through `workspace/symbol`, and the shape it needs is the category's `search` hook
  // rather than `items`: the server does the matching, over a workspace far too large to hand over
  // whole, and a rust-analyzer answer to an empty query is empty by design.

  /** LSP symbol kinds that belong under **Types** — what <kbd>Ctrl</kbd>+<kbd>N</kbd> means in a
   *  language with no classes: a struct, an enum, a trait, an impl, a type alias.
   *
   *  `interface` and `object` are here as well as `trait` and `impl` because the vocabulary is the
   *  server's language's: a Rust trait arrives as `trait`, and a TypeScript interface as
   *  `interface`. */
  const TYPE_KINDS = new Set([
    'struct', 'enum', 'interface', 'trait', 'class', 'namespace', 'module', 'object', 'impl',
    'type alias',
  ]);

  /**
   * One `workspace/symbol` search, kept to `kinds` (or to everything else when `invert`).
   *
   * The two tabs draw a row differently, and the difference is not decoration:
   *
   *   * **Types** — every row is a type, so a shape says nothing and the *letter* is the whole
   *     answer: `S` struct, `T` trait, `E` enum, in the same lettered ring a Java class wears.
   *   * **Symbols** — a mixed list, so the distinction worth drawing is function versus constant
   *     versus field, and that one is shape.
   */
  async function lspSymbolSearch(
    query: string,
    kinds: Set<string>,
    invert: boolean,
  ): Promise<NavigateItem[]> {
    const root = projectStore.project?.root;
    // A one-character query against a large workspace is a lot of rows for no discrimination, and
    // the server has to walk its whole index to produce them.
    if (!root || query.trim().length < 2) return [];
    const hits = await lspWorkspaceSymbols(root, query.trim()).catch(() => []);
    return hits
      .filter((s) => (invert ? !kinds.has(s.kind) : kinds.has(s.kind)))
      .map((s, i) => {
        const glyph = kindGlyph(s.kind);
        return at(
          {
            id: `${s.name}@${s.file}:${s.line}:${i}`,
            name: s.name,
            detail: s.detail ?? relativeTo(root, s.file),
            ...(invert
              ? { icon: glyph.icon, iconProps: { color: glyph.color } }
              : { icon: SymbolKindIcon, iconProps: { kind: s.kind } }),
            tag: s.kind,
            onOpen: () => go(s.file, s.line),
          },
          s.file,
          s.line,
        );
      });
  }

  /** Whether this project's intelligence comes from a language server rather than the Java index.
   *
   *  Gated on the project *kind* rather than on a server being up: the categories are rebuilt from
   *  this, and swapping them as a server starts and stops would change what <kbd>Ctrl</kbd>+<kbd>N</kbd>
   *  does mid-session. A Java project is untouched by any of this. */
  const lspBacked = $derived(projectStore.isCargo);

  const categories = $derived.by<NavigateCategory[]>(() => {
    // Read so that changing the reach produces a NEW category list. That identity is what the
    // overlay re-pulls its items on, and the suppliers close over `searchRoots` rather than
    // taking it as an argument — without this the workspace toggle would flip and change
    // nothing, which is the worst way for a control to be wrong.
    void searchRoots;
    return [
    lspBacked
      ? {
          id: 'classes',
          // Not "Classes": the things it finds are structs, enums and traits, and a header that
          // called them classes would be describing a language this project is not written in.
          label: 'Types',
          emptyMessage: 'Type at least two characters to search types.',
          preview: previewOf,
          search: (q) => lspSymbolSearch(q, TYPE_KINDS, false),
        }
      : {
          id: 'classes',
          label: 'Classes',
          emptyMessage: 'No classes indexed yet.',
          facetLabel: 'Module',
          preview: previewOf,
          sources: sourcesFor(projectClasses, classpathClasses, 'classes'),
        },
    {
      id: 'files',
      label: 'Files',
      emptyMessage: 'No files in this project.',
      // The same dimension, for the same reason: on a reactor, "the `pom.xml` of *this* module"
      // is as common a question as "the `OrderDao` of this module".
      facetLabel: 'Module',
      preview: previewOf,
      sources: sourcesFor(projectFiles, classpathFiles, 'files'),
    },
    lspBacked
      ? {
          id: 'symbols',
          label: 'Symbols',
          emptyMessage: 'Type at least two characters to search symbols.',
          preview: previewOf,
          // Everything that is not a type: functions, methods, constants, statics, fields. The
          // split mirrors the Java pair — Classes finds the declaration, Symbols finds the member.
          search: (q) => lspSymbolSearch(q, TYPE_KINDS, true),
        }
      : {
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
    ];
  });

  /**
   * Which source the box opens on.
   *
   * The default stays **Project**, and the setting is what moves it: with a two-letter query the
   * classpath reaches a hundred thousand things nobody here wrote, and what you were looking for
   * is almost always one of your own. What the setting no longer does is decide whether the
   * classpath is reachable at all — that is one pick away now, in front of you, instead of a
   * round trip through Settings.
   */
  const initialSource = $derived(bennuSettingsStore.searchDependencies ? 'both' : 'project');

</script>

<NavigateTo
  {categories}
  {initialCategory}
  {initialSource}
  initialQuery={bennuUiStore.navInitial}
  title="Go to"
  requireQuery
  {onClose}
>
  {#snippet fieldActions()}
    {#if projectStore.hasWorkspace}
      <ToggleButton
        pressed={workspace}
        icon={FolderTree}
        ariaLabel="Search the whole workspace"
        title={workspace
          ? 'Searching every project in the workspace — click for this project only'
          : 'Search every project in the workspace'}
        onclick={() => (workspace = !workspace)}
      />
    {/if}
  {/snippet}
</NavigateTo>
