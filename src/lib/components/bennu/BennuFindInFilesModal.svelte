<script lang="ts">
  /**
   * Find in project (Ctrl+Shift+F) — the results list beside a **preview of the file**.
   *
   * The list alone answers "where does this string occur"; it does not answer the question
   * you actually opened it with, which is "is THIS the occurrence I meant". A one-line
   * excerpt is not enough to tell four identical `if (rs.next())` apart — the lines around
   * it are. So the selected hit is shown in context on the right, the way IntelliJ does it,
   * and walking the list with ↑/↓ re-reads it as you go. Nothing is opened until you press
   * Enter, which is what makes browsing a hundred hits cheap.
   *
   * Runs the backend recursive grep (`bennu_find_in_files`) **progressively**: each search
   * gets a fresh `searchId`, and results stream back as `arbor://bennu/find-progress` events.
   * A `done` event ends the spinner. Events tagged with a superseded id are ignored, so a newer
   * query is never clobbered by a slower older scan. Debounced (~250ms). When the BE is absent
   * the call rejects and we render a graceful empty state.
   *
   * **Two things keep that from locking the window**, and both are about the cost per batch
   * rather than the scan itself — the backend flushes at every file boundary, so a legacy
   * project sends thousands of them:
   *
   *   * batches land in a non-reactive buffer and reach the list on a timer (~12 renders a
   *     second however fast they arrive), instead of re-assigning `hits` — a copy of everything
   *     so far — on each one;
   *   * the list is **windowed** (`VirtualList`), so five thousand matches are ~40 rows of DOM.
   *     That is what the grouping is flattened for: a window is `scrollTop / rowHeight`, which
   *     needs one array with one height, and a file header is just another row kind.
   *
   * The header row carries everything that decides **what is searched**: how many projects (the
   * pills, present only when there is a workspace), **whose text** (the source picker — the
   * project, its dependency jars, or both), and the two narrowings, the **module** and the
   * **file mask** (`*.java`, `*.jsp,*.tag`). The narrowings filter what the scan returned rather
   * than what it scans — the BE takes neither, and re-running the walk for a test this side
   * answers instantly would be slower AND less responsive — and both are remembered per project.
   *
   * A hit inside a jar names its **artifact** and is **tinted**, because a result you can only
   * read is not the same kind of answer as one you can go and change.
   *
   * Keyboard-first: the query field auto-focuses and keeps focus; ↑/↓ move the highlighted
   * hit (flattened across groups), PageUp/PageDown jump, Enter opens it and closes, Esc
   * cancels (Modal owns Esc). Replace is intentionally out of scope (no affordance).
   */
  import { Search, CornerDownLeft, Filter, FolderTree } from 'lucide-svelte';
  import BennuFileIcon from './BennuFileIcon.svelte';
  import { untrack } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import Modal from '$lib/components/shared/Modal.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import ToggleButton from '$lib/components/shared/ui/ToggleButton.svelte';
  import VirtualList from '$lib/components/shared/ui/VirtualList.svelte';
  import CodePreview from '$lib/components/shared/ui/CodePreview.svelte';
  import { languageForPath } from './languages';
  import { moduleIndex } from './modules';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
  import { findInFiles, getFindPrefs, readFile, setFindPrefs, type FindSources } from '$lib/ipc/bennu';
  import { isJarEntry, openLibraryFile } from '$lib/ipc/bennu/library';
  import type { FindHit } from '$lib/types/bennu';
  import { tooltip } from '$lib/actions/tooltip';

  let { onClose }: { onClose: () => void } = $props();

  // Seed from the editor selection when opened with one highlighted (bennuUiStore.findInitial),
  // else empty. Read once at mount — the value is set right before the modal opens.
  let query = $state(bennuUiStore.findInitial);
  /**
   * The file mask — **remembered per project**, unlike the query.
   *
   * A query is a question you asked once; a mask is a shape of project ("on this tree I only
   * ever mean the JSPs"), and re-typing it at every opening is the friction that makes people
   * stop using the filter at all. It lives in `<repo>/.arbor/bennu/config.toml`, beside the run
   * configurations, because the answer differs per project.
   */
  let mask = $state('');
  /** Until the stored preferences have landed, a change is the loader's and not the user's —
   *  saving it back would write the empty defaults over what was on disk. */
  let prefsLoaded = $state(false);
  let regex = $state(false);
  let caseSensitive = $state(false);
  let wholeWord = $state(false);

  // ── where the scan reaches ───────────────────────────────────────────────────
  /**
   * Also scan the other projects of the workspace.
   *
   * A **toggle**, like match case beside it, rather than a segmented control: it is one bit, it
   * only exists where there is a workspace to reach into, and a two-item pill strip costs a
   * quarter of the header row to say what a pressed key says.
   */
  let workspace = $state(false);
  /** If the last workspace closes while the modal is open, the toggle means nothing — read it
   *  through this rather than trusting the bit. */
  const searchWorkspace = $derived(workspace && projectStore.hasWorkspace);

  /**
   * Whose text is read — the same three sources the go-to overlay offers, in the same words.
   *
   * Never persisted, unlike the mask and the module: every candidate jar entry has to be
   * decompressed to be read, so it is a cost you opt into for the question you are asking now,
   * not one you turn on and forget about. The default follows the *Search the dependencies too*
   * setting, so a preference for reaching the classpath is honoured without hiding the control.
   */
  const SOURCES: { value: string; label: string }[] = [
    { value: 'project', label: 'Project' },
    { value: 'project_and_dependencies', label: 'Project & dependencies' },
    { value: 'dependencies', label: 'Dependencies' },
  ];
  // Held as a plain string because that is what the picker binds; narrowed once, where it is
  // sent, rather than cast at every read.
  // svelte-ignore state_referenced_locally
  let sources = $state<string>(
    bennuSettingsStore.searchDependencies ? 'project_and_dependencies' : 'project',
  );
  const findSources = $derived(sources as FindSources);
  /** The jars alone have no project to reach out of, so the toggle goes away with it rather than
   *  sitting there deciding nothing. */
  const workspaceApplies = $derived(sources !== 'dependencies' && projectStore.hasWorkspace);

  // ── which module ─────────────────────────────────────────────────────────────
  /** The module lookup, shared with the go-to overlay (`./modules`). */
  const modules = $derived(moduleIndex(projectStore.project));
  /** The chosen module, or `''` for all of them. Filters what the scan returned, like the mask:
   *  a module is a path prefix, and re-walking the tree for a test this side answers instantly
   *  would be slower AND less responsive. */
  let moduleFilter = $state('');

  let hits = $state<FindHit[]>([]);
  let loading = $state(false);
  let errored = $state(false);
  let capped = $state(false);
  let sel = $state(0);
  // The field keeps the focus throughout: refining a query after looking at the results is the
  // normal case, so the arrows drive the list without ever leaving the input.
  //
  // `data-modal-autofocus` on the input is what makes it focused *on opening* — and it is not
  // belt-and-braces for the effect below. `Modal` runs its own initial-focus pass in a microtask,
  // which is after the child effects, so it wins any race with them; its guess is "the first
  // focusable that is not in the header", and this modal's body opens with the source picker and the
  // file mask. Without the attribute the caret landed on a dropdown and the query had to be clicked.
  let field = $state<HTMLInputElement | null>(null);
  $effect(() => { field?.focus(); });

  function baseName(p: string): string { return p.split(/[\\/]/).pop() ?? p; }

  /** A path shown relative to the project root — the part that tells two files apart. */
  function relPath(p: string): string { return modules.relative(p); }

  // ── Progressive search (streamed via `arbor://bennu/find-progress`) ───────────
  // Each run mints a fresh `currentId`; the event listener appends only the batches
  // tagged with it, so a slower superseded scan can never clobber a newer query.
  let seq = 0;
  let currentId = '';
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  // The BE payload shape (`{ id, hits?, done?, capped? }`).
  interface FindProgress { id: string; hits?: FindHit[]; done?: boolean; capped?: boolean }

  /**
   * Batches that have landed but are not on screen yet.
   *
   * A deliberately **non-reactive** array, and the reason is the difference between a search you
   * can read as it runs and one that locks the window. The backend flushes at every file
   * boundary, so a legacy project sends thousands of batches; assigning `hits` on each one costs
   * a copy of everything so far — quadratic overall — and re-runs the grouping, the whole list's
   * render and the preview's read every time. The main thread never comes up for air, which is
   * why the list felt frozen and the arrows dead until the scan ended.
   *
   * So batches accumulate here and reach the list on a timer: at most ~12 renders a second
   * however fast the results arrive.
   */
  let pending: FindHit[] = [];
  let flushTimer: ReturnType<typeof setTimeout> | undefined;
  const FLUSH_MS = 80;

  function publish() {
    flushTimer = undefined;
    if (!pending.length) return;
    hits = hits.concat(pending);
    pending = [];
  }

  function schedulePublish() {
    if (flushTimer === undefined) flushTimer = setTimeout(publish, FLUSH_MS);
  }

  /** Drop anything in flight — a new search, or the modal going away. */
  function stopPublishing() {
    if (flushTimer !== undefined) clearTimeout(flushTimer);
    flushTimer = undefined;
    pending = [];
  }

  $effect(() => {
    let un: UnlistenFn | undefined;
    void listen<FindProgress>('arbor://bennu/find-progress', (e) => {
      const p = e.payload;
      if (p.id !== currentId) return; // a superseded search — ignore
      if (p.hits && p.hits.length) {
        pending.push(...p.hits);
        schedulePublish();
      }
      if (p.capped) capped = true;
      // The terminal event publishes at once: waiting out the timer to show the last few would
      // leave the spinner off and the count short for no reason.
      if (p.done) {
        if (flushTimer !== undefined) clearTimeout(flushTimer);
        publish();
        loading = false;
      }
    }).then((fn) => { un = fn; });
    return () => { un?.(); stopPublishing(); };
  });

  function runSearch() {
    const root = projectStore.project?.root;
    const q = query.trim();
    const id = `find-${++seq}`;
    currentId = id;
    stopPublishing(); // the old scan's tail must not land in the new list
    hits = [];
    sel = 0;
    capped = false;
    if (!root || q.length < 2) {
      loading = false;
      errored = false;
      return;
    }
    loading = true;
    errored = false;
    // Workspace scope: also scan the OTHER member projects (the BE streams them into the same
    // search). Anything else leaves `extraRoots` empty.
    const extraRoots = workspaceApplies && searchWorkspace
      ? projectStore.workspaceProjects.map((p) => p.root).filter((r) => r !== root)
      : [];
    findInFiles(
      root,
      q,
      { regex, caseSensitive, wholeWord, extraRoots, sources: findSources },
      id,
    ).catch(() => {
      if (id !== currentId) return;
      // BE absent / rejected query (e.g. bad regex) → graceful empty state.
      hits = [];
      loading = false;
      errored = true;
    });
  }

  // Re-run on any input change (query text or a toggle), debounced. The mask and the module are
  // NOT dependencies — they filter what came back, so re-scanning for either would be pure waste.
  $effect(() => {
    void query; void regex; void caseSensitive; void wholeWord; void searchWorkspace; void sources;
    void projectStore.project;
    if (debounceTimer !== undefined) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(runSearch, 250);
    return () => { if (debounceTimer !== undefined) clearTimeout(debounceTimer); };
  });

  // ── The narrowing, loaded per project and saved once it settles ──────────────
  $effect(() => {
    const root = projectStore.project?.root;
    if (!root) return;
    let live = true;
    void (async () => {
      try {
        const prefs = await getFindPrefs(root);
        if (!live) return;
        mask = prefs.mask;
        // A module the build no longer has is a filter that hides every result with nothing on
        // screen to explain it — a renamed module would otherwise make the search look broken.
        moduleFilter = prefs.module && modules.modules.includes(prefs.module) ? prefs.module : '';
      } catch {
        // No stored preferences is not a failure — it is a project nobody has filtered yet.
      } finally {
        if (live) prefsLoaded = true;
      }
    })();
    return () => { live = false; };
  });

  /** Written after the controls settle, not per keystroke: `*.j`, `*.ja`, `*.jav` are three
   *  writes of a mask nobody meant. */
  const PREFS_SAVE_MS = 600;
  $effect(() => {
    const prefs = { mask, module: moduleFilter };
    const root = projectStore.project?.root;
    if (!prefsLoaded || !root) return;
    const timer = setTimeout(() => void setFindPrefs(root, prefs).catch(() => {}), PREFS_SAVE_MS);
    return () => clearTimeout(timer);
  });

  // ── What the results are narrowed by ─────────────────────────────────────────
  /** `*.java, *.jsp` → a test over the file name. An empty / all-blank mask passes everything. */
  const maskTest = $derived.by<(file: string) => boolean>(() => {
    const parts = mask.split(/[,;\s]+/).map((p) => p.trim()).filter(Boolean);
    if (!parts.length) return () => true;
    const res = parts.map((p) => {
      const body = p.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*').replace(/\?/g, '.');
      return new RegExp(`^${body}$`, 'i');
    });
    return (file: string) => {
      const name = baseName(file);
      return res.some((re) => re.test(name));
    };
  });

  /** The chosen module → a test over the file's path. A hit with no module — the root `pom.xml`,
   *  anything inside a dependency jar — fails it, which is the honest answer: it is not in that
   *  module. */
  const moduleTest = $derived.by<(file: string) => boolean>(() => {
    const wanted = moduleFilter;
    if (!wanted) return () => true;
    const index = modules;
    return (file: string) => index.moduleOf(file) === wanted;
  });

  const shown = $derived(hits.filter((h) => maskTest(h.file) && moduleTest(h.file)));

  /** What is hiding results, phrased for the empty state — "none of the 412 matches are here" is
   *  only useful if it also says where *here* is. */
  const narrowing = $derived.by<string>(() => {
    const bits: string[] = [];
    if (mask.trim()) bits.push(`files matching “${mask.trim()}”`);
    if (moduleFilter) bits.push(`module ${moduleFilter}`);
    return bits.join(' and ');
  });

  // ── Rows: grouping by file, flattened so the list can be windowed ────────────
  /**
   * One entry per rendered line — a file header or one of its hits.
   *
   * Flat rather than nested because the list is **virtualized**: a window is
   * `scrollTop / rowHeight`, which needs one array with one height. Grouping survives as a row
   * kind, so the result reads exactly as before while only the ~40 lines on screen exist in the
   * DOM. A legacy project answering with five thousand hits used to build five thousand nodes
   * and re-lay them out on every batch that landed.
   */
  type Row =
    | {
        kind: 'file';
        key: string;
        name: string;
        dir: string;
        file: string;
        /** Where the file is from: the module on a reactor, the artifact for a jar entry. */
        origin: string;
        /** Inside a dependency — read-only, and drawn as such. */
        external: boolean;
        count: number;
      }
    | { kind: 'hit'; key: string; hit: FindHit; idx: number; external: boolean };

  /**
   * A jar hit's path split into the artifact and the entry inside it.
   *
   * `<jar>!/<entry>` is a path with no directory of its own, so the plain relative-path
   * treatment leaves an absolute `~/.m2/repository/...` string where the folder should be. The
   * artifact is what you actually want to read there — *which library says this* — and the entry
   * is what tells two `web.xml`s apart.
   */
  function jarParts(file: string): { artifact: string; entry: string } | null {
    const cut = file.indexOf('!/');
    if (cut < 0) return null;
    return { artifact: baseName(file.slice(0, cut)), entry: file.slice(cut + 2) };
  }

  const rows = $derived.by<Row[]>(() => {
    const out: Row[] = [];
    let openFile: string | null = null;
    let header: Extract<Row, { kind: 'file' }> | null = null;
    shown.forEach((hit, idx) => {
      const jar = jarParts(hit.file);
      if (hit.file !== openFile) {
        const rel = jar ? jar.entry : relPath(hit.file);
        const cut = rel.lastIndexOf('/');
        header = {
          kind: 'file',
          key: `f:${hit.file}`,
          file: hit.file,
          name: baseName(rel),
          dir: cut < 0 ? '' : rel.slice(0, cut),
          origin: jar ? jar.artifact : (modules.moduleOf(hit.file) ?? ''),
          external: !!jar,
          count: 0,
        };
        out.push(header);
        openFile = hit.file;
      }
      if (header) header.count += 1;
      out.push({
        kind: 'hit',
        key: `h:${hit.file}:${hit.line}:${hit.col}:${idx}`,
        hit,
        idx,
        external: !!jar,
      });
    });
    return out;
  });

  /** How many files the hits fall in — the header count, without a second pass. */
  const fileCount = $derived(rows.reduce((n, r) => (r.kind === 'file' ? n + 1 : n), 0));

  /** Where the selected hit sits among the ROWS — headers shift it, and the window scrolls by
   *  row index, not by hit index. */
  const selRow = $derived(rows.findIndex((r) => r.kind === 'hit' && r.idx === sel));

  /** Both kinds are laid out to this height; see `VirtualList`. */
  const ROW_H = 22;

  // Keep the selection in-range as results (or the mask) change.
  $effect(() => { if (sel >= shown.length) sel = Math.max(0, shown.length - 1); });

  const current = $derived<FindHit | undefined>(shown[sel]);
  /** The selected hit's identity. The preview keys off **this**, not off `current`: every batch
   *  that lands rebuilds `shown`, so an effect depending on the object would re-read the file at
   *  every flush while the selection sat still. */
  const currentKey = $derived(current ? `${current.file}:${current.line}:${current.col}` : '');

  // ── Preview of the selected hit ──────────────────────────────────────────────
  /** Files already read, so walking a file's hits re-reads nothing. Cleared per opening. */
  const fileCache = new Map<string, string>();
  /** The selected hit's file, **whole** — the column is a real read-only editor, so it wants the
   *  document its line numbers and its multi-line constructs come from. */
  let previewText = $state('');
  let previewFile = $state('');
  let previewError = $state(false);

  /** The path over the preview. A jar entry has no project-relative form, so it is named the way
   *  it is actually addressed: the artifact, then the entry inside it. */
  const previewLabel = $derived.by<string>(() => {
    const jar = jarParts(previewFile);
    return jar ? `${jar.artifact} — ${jar.entry}` : relPath(previewFile);
  });

  $effect(() => {
    // Tracked: the selection's identity. Read untracked: the hit itself — so a batch landing
    // does not count as "the selection changed" and re-read the file.
    void currentKey;
    const hit = untrack(() => current);
    const root = untrack(() => projectStore.project?.root);
    if (!hit || !root) { previewText = ''; previewFile = ''; return; }
    let live = true;
    void (async () => {
      let text = fileCache.get(hit.file);
      if (text === undefined) {
        try {
          // Keyed by the hit's own id, not the resolved path — a jar entry is extracted once
          // and then read like anything else, and the cache must not miss on the second visit.
          const path = await resolveHitPath(hit.file);
          if (!path) throw new Error('unresolvable');
          text = (await readFile(root, path)).text;
          fileCache.set(hit.file, text);
        } catch {
          if (!live) return;
          previewError = true;
          previewText = '';
          previewFile = hit.file;
          return;
        }
      }
      if (!live) return;
      previewError = false;
      previewFile = hit.file;
      previewText = text;
    })();
    return () => { live = false; };
  });

  // ── Match highlighting ───────────────────────────────────────────────────────
  // Split a line around the first match of the query so it can be emphasised. For regex we
  // do a lenient case-insensitive first-match; a bad pattern just yields no highlight (the
  // row still renders plainly).
  interface Segment { text: string; hit: boolean; }
  const matcher = $derived.by<RegExp | null>(() => {
    const q = query.trim();
    if (!q) return null;
    try {
      const flags = caseSensitive ? '' : 'i';
      if (regex) return new RegExp(q, flags);
      const escaped = q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      return new RegExp(wholeWord ? `\\b${escaped}\\b` : escaped, flags);
    } catch {
      return null;
    }
  });

  function segments(text: string): Segment[] {
    const re = matcher;
    if (!re) return [{ text, hit: false }];
    const m = re.exec(text);
    if (!m || m.index < 0 || m[0].length === 0) return [{ text, hit: false }];
    return [
      { text: text.slice(0, m.index), hit: false },
      { text: text.slice(m.index, m.index + m[0].length), hit: true },
      { text: text.slice(m.index + m[0].length), hit: false },
    ];
  }

  async function openHit(h: FindHit) {
    const path = await resolveHitPath(h.file);
    if (!path) return;
    await projectStore.openFile(path);
    bennuUiStore.requestGoto(h.line);
    onClose();
  }

  /**
   * A hit's file as something the editor can open.
   *
   * A hit in the project already is one. A hit inside a dependency is `<jar>!/<entry>` — text
   * in a zip, with no path of its own — so it is extracted to the read-only cache first. Doing
   * it here rather than at search time means the three thousand jar hits nobody clicked cost
   * nothing.
   */
  async function resolveHitPath(file: string): Promise<string | null> {
    if (!isJarEntry(file)) return file;
    const root = projectStore.project?.root;
    if (!root) return null;
    try {
      return await openLibraryFile(root, file);
    } catch {
      return null;
    }
  }

  function move(delta: number) {
    if (!shown.length) return;
    // No scrolling here: the list is windowed and follows `selRow` itself — an off-screen row
    // has no element to scroll to, which is precisely the case that needs scrolling.
    sel = Math.min(Math.max(sel + delta, 0), shown.length - 1);
  }

  function onKey(e: KeyboardEvent) {
    switch (e.key) {
      case 'ArrowDown': e.preventDefault(); move(1); break;
      case 'ArrowUp': e.preventDefault(); move(-1); break;
      case 'PageDown': e.preventDefault(); move(8); break;
      case 'PageUp': e.preventDefault(); move(-8); break;
      case 'Enter': {
        e.preventDefault();
        if (current) void openHit(current);
        break;
      }
      default: break;
    }
  }

  const hasQuery = $derived(query.trim().length >= 2);
</script>

<!-- No title bar, like the go-to modals: the field is focused the moment it opens and its
     placeholder already says what this is. A chrome row whose only job is to repeat that costs
     a row of the results. -->
<Modal {onClose} width="1000px" height="620px" padBody={false} ariaLabel="Find in project">
  <div class="ff" onkeydown={onKey} role="presentation">
    <!--
      The header row, the go-to modals' one: everything that decides WHAT IS SEARCHED, and
      nothing that decides how. The pills say how many projects, the source picker says whose
      text, the module and the mask narrow what comes back, and the count in between is the
      answer to all four. None of it belongs beside the field, which is for the query and for
      how the query is read.
    -->
    <div class="ff-bar">
      <span class="ff-sub-spacer"></span>
      {#if capped}<span class="ff-cap">capped</span>{/if}
      {#if hasQuery && shown.length}
        <span class="ff-count">
          {shown.length} match{shown.length === 1 ? '' : 'es'} in {fileCount} file{fileCount === 1 ? '' : 's'}
          {#if shown.length !== hits.length}<span class="ff-count-mask">of {hits.length}</span>{/if}
        </span>
      {/if}
      <Select
        value={sources}
        options={SOURCES}
        size="sm"
        highlight={sources !== 'project'}
        ariaLabel="Source"
        onchange={(v) => (sources = v)}
      />
      {#if modules.sorted.length}
        <Select
          value={moduleFilter}
          options={[{ value: '', label: 'All modules' }, ...modules.sorted.map((m) => ({ value: m, label: m }))]}
          size="sm"
          highlight={!!moduleFilter}
          searchable={modules.sorted.length > 12}
          searchPlaceholder="Filter modules…"
          ariaLabel="Module"
          onchange={(v) => (moduleFilter = v)}
        />
      {/if}
      <span class="ff-mask">
        <Filter size={12} />
        <Input bind:value={mask} placeholder="*.java, *.jsp" ariaLabel="File mask" />
      </span>
    </div>

    <div class="ff-search">
      <Search size={15} />
      <input
        bind:this={field}
        bind:value={query}
        class="ff-field"
        type="text"
        spellcheck="false"
        autocomplete="off"
        placeholder="Find in project…"
        aria-label="Find in project"
        data-modal-autofocus
      />
      {#if loading}<Spinner size={13} />{/if}
      <!-- The keys of a search bar: how a match is judged, and how far out of this project it
           looks. All four are one bit each and all four re-run the search on the spot, which is
           what a pressed key says and a dropdown does not. -->
      <div class="ff-toggles">
        <ToggleButton
          pressed={caseSensitive}
          label="Aa"
          title="Match case"
          onclick={() => (caseSensitive = !caseSensitive)}
        />
        <ToggleButton
          pressed={wholeWord}
          label="W"
          title="Whole word"
          onclick={() => (wholeWord = !wholeWord)}
        />
        <ToggleButton
          pressed={regex}
          label=".*"
          title="Regular expression"
          onclick={() => (regex = !regex)}
        />
        {#if workspaceApplies}
          <ToggleButton
            pressed={searchWorkspace}
            icon={FolderTree}
            ariaLabel="Search the whole workspace"
            title={searchWorkspace
              ? 'Searching every project in the workspace — click for this project only'
              : 'Search every project in the workspace'}
            onclick={() => (workspace = !workspace)}
          />
        {/if}
      </div>
    </div>

    {#if !projectStore.project}
      <EmptyState message="Open a project to search its files." />
    {:else if !hasQuery}
      <EmptyState message="Type at least 2 characters to search." />
    {:else if shown.length === 0}
      {#if loading}
        <div class="ff-loading"><Spinner size="sm" label="Searching…" /></div>
      {:else if hits.length && narrowing}
        <EmptyState message={`${hits.length} match(es), none in ${narrowing}.`} />
      {:else}
        <EmptyState message={errored ? 'Search is unavailable for this project.' : `No matches for “${query.trim()}”.`} />
      {/if}
    {:else}
      <div class="ff-split">
        <VirtualList
          items={rows}
          rowHeight={ROW_H}
          getKey={(r) => r.key}
          scrollTo={selRow}
          class="ff-list"
          ariaLabel="Matches"
        >
          {#snippet row({ item }: { item: Row })}
            {#if item.kind === 'file'}
              <div class="ff-group-head" class:ext={item.external} use:tooltip={item.file}>
                <!-- The tree's own icon rule, not a stand-in for it: a `.java` wears the kind it
                     declares and everything else its file type, exactly as in the project tree
                     and the go-to modals. One generic document glyph for every result was the
                     last place in Bennu still answering "what kind of file is this" with a
                     shrug. -->
                <BennuFileIcon size={13} path={item.file} />
                <span class="ff-group-name">{item.name}</span>
                {#if item.dir}<span class="ff-group-dir">{item.dir}</span>{/if}
                <!-- Which module, or which artifact — the answer to "is this mine" in the one
                     place your eye already goes for the count. -->
                {#if item.origin}<span class="ff-group-origin">{item.origin}</span>{/if}
                <span class="ff-group-count">{item.count}</span>
              </div>
            {:else}
              <button
                class="ff-hit"
                class:ext={item.external}
                class:sel={item.idx === sel}
                onclick={() => openHit(item.hit)}
                onmousemove={() => (sel = item.idx)}
              >
                <span class="ff-loc">{item.hit.line}:{item.hit.col}</span>
                <span class="ff-line-text">{#each segments(item.hit.preview) as s, i (i)}{#if s.hit}<mark class="ff-mark">{s.text}</mark>{:else}{s.text}{/if}{/each}</span>
              </button>
            {/if}
          {/snippet}
        </VirtualList>

        <div class="ff-preview">
          {#if previewError}
            <p class="ff-pv-note">This file can’t be previewed.</p>
          {:else if previewText && current}
            <div class="ff-pv-head" use:tooltip={previewFile}>{previewLabel}</div>
            <div class="ff-pv-body">
              <!-- The real editor, so the context is coloured exactly as the buffer is — the
                   same grammar, not a second highlighter that agrees with it on Java and not on
                   XML. The whole file, so the line numbers are the file's own. -->
              <CodePreview
                text={previewText}
                language={languageForPath(previewFile)}
                activeLine={current.line}
              />
            </div>
          {:else}
            <p class="ff-pv-note">Reading…</p>
          {/if}
        </div>
      </div>

      <div class="ff-foot">
        <Kbd keys={["↑"]} size="sm" /><Kbd keys={["↓"]} size="sm" /><span>move</span>
        <span class="ff-foot-open"><CornerDownLeft size={11} /> open</span>
        <span class="ff-sub-spacer"></span>
        <Kbd keys={["Esc"]} size="sm" /><span>close</span>
      </div>
    {/if}
  </div>
</Modal>

<style>
  .ff { display: flex; flex-direction: column; height: 100%; min-height: 0; }

  /* The header row, the go-to modals' one: the scope pills on the left, the count and the two
     narrowings on the right. It is the topmost row — the title bar is gone, the way it is on the
     go-to modals. */
  .ff-bar {
    display: flex; align-items: center; gap: 8px;
    padding: 7px 12px; flex-shrink: 0;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-2xs); color: var(--text-muted);
  }

  .ff-search {
    display: flex; align-items: center; gap: 8px;
    padding: 11px 14px; flex-shrink: 0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .ff-search > :global(svg) { color: var(--text-disabled); flex-shrink: 0; }
  .ff-field {
    flex: 1; min-width: 0;
    background: none; border: none; outline: none;
    color: var(--text-primary); font-size: var(--font-size-lg);
  }
  .ff-field::placeholder { color: var(--text-disabled); }

  .ff-toggles { display: flex; gap: 4px; flex-shrink: 0; }

  /* Sized like the module picker beside it: wide enough for a couple of globs, never wide enough
     to compete with the query field below. */
  .ff-mask { display: flex; align-items: center; gap: 6px; min-width: 0; width: 190px; flex-shrink: 0; }
  .ff-mask :global(svg) { color: var(--text-disabled); flex-shrink: 0; }
  .ff-sub-spacer { flex: 1; }
  /* The count sits between the pills and the two narrowings and must not wrap or be squeezed —
     it is the sentence that says what all three of them produced. */
  .ff-count { white-space: nowrap; flex-shrink: 0; }
  .ff-count-mask { color: var(--text-disabled); margin-left: 4px; }
  .ff-cap { color: var(--warning); }

  .ff-loading { display: flex; align-items: center; justify-content: center; padding: 24px; }

  /* Results left, the selected hit in context right — the split is the point. */
  .ff-split { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
  /* `:global` because the class rides a prop onto VirtualList's own element — the last resort
     CLAUDE.md allows, and the alternative (a wrapper div) would break the grid row sizing. */
  :global(.ff-list) { border-right: 1px solid var(--border-subtle); }

  /* Both row kinds are laid out to the SAME height (`ROW_H`), which is what lets one windowed
     list hold a grouped result. Padding is vertical-centred rather than asymmetric so the two
     read as different rows without measuring differently. */
  .ff-group-head {
    display: flex; align-items: center; gap: 6px; height: 100%;
    padding: 0 14px; color: var(--text-secondary);
    font-size: var(--font-size-xs); font-weight: 600;
  }
  .ff-group-head :global(svg) { align-self: center; color: var(--text-muted); flex-shrink: 0; }
  .ff-group-name { flex-shrink: 0; }
  .ff-group-dir {
    flex: 1; min-width: 0;
    font-family: var(--font-code); font-size: var(--font-size-3xs); font-weight: 400;
    color: var(--text-disabled);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left;
  }
  .ff-group-count {
    margin-left: auto; font-size: var(--font-size-3xs); font-weight: 500;
    color: var(--text-disabled);
    background: var(--bg-elevated); border-radius: 99px; padding: 0 6px;
  }
  /* `margin-left: auto` on the count would push this to the left edge if it came after it, so
     the origin sits before it and both stay right-aligned. */
  .ff-group-origin {
    flex-shrink: 0; max-width: 200px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    margin-left: auto;
    font-size: var(--font-size-3xs); font-weight: 400;
    color: var(--text-muted);
    background: var(--bg-elevated); border-radius: var(--radius-sm); padding: 0 5px;
  }
  .ff-group-origin + .ff-group-count { margin-left: 0; }

  /* Not the user's own — a hit inside a dependency, which you can read and cannot change. Same
     hue and same reasoning as the editor's external tabs and the go-to overlay's library rows:
     not an error, but not one of your files either. The bar on the left edge is what makes a
     run of them legible as a block while scrolling past. */
  .ff-group-head.ext, .ff-hit.ext {
    background: color-mix(in srgb, var(--warning) 7%, transparent);
    box-shadow: inset 2px 0 0 color-mix(in srgb, var(--warning) 45%, transparent);
  }
  .ff-hit.ext:hover { background: color-mix(in srgb, var(--warning) 12%, transparent); }
  /* The selection has to win over the tint, and equal specificity would leave that to source
     order — the kind of thing that breaks the day a rule moves. */
  .ff-hit.ext.sel, .ff-hit.ext.sel:hover {
    background: var(--bg-selected);
    box-shadow: inset 2px 0 0 var(--warning);
  }

  .ff-hit {
    display: flex; align-items: center; gap: 10px; height: 100%;
    width: 100%; text-align: left;
    padding: 0 14px 0 30px; background: transparent; border: none; cursor: pointer;
  }
  .ff-hit.sel { background: var(--bg-selected); }
  .ff-hit:hover { background: var(--bg-hover); }
  .ff-hit.sel:hover { background: var(--bg-selected); }
  .ff-loc {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-disabled);
    flex-shrink: 0; min-width: 44px;
  }
  .ff-line-text {
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0;
  }
  /* A search hit has to be findable at a glance in a wall of monospace, which `--accent-subtle`
     (15% alpha) is not — it reads as a faint tint you have to already know is there. This is the
     one place in the modal that earns a solid fill: it is the answer to the question asked. */
  .ff-mark {
    background: var(--accent);
    color: var(--bg-base);
    border-radius: 2px;
    padding: 0 2px;
    font-weight: 600;
  }
  /* On the selected row the fill would fight the selection band, so the hit inverts instead. */
  .ff-hit.sel .ff-mark { background: var(--accent-hover); }

  .ff-preview { min-height: 0; display: flex; flex-direction: column; background: var(--bg-base); }
  .ff-pv-head {
    flex-shrink: 0; padding: 6px 12px;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
    border-bottom: 1px solid var(--border-subtle);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left;
  }
  /* The preview is a read-only editor (`CodePreview`), which owns its own gutter, colouring and
     scrolling — so this is only the box it fills. */
  .ff-pv-body { flex: 1; min-height: 0; }
  .ff-pv-note { padding: 14px; font-size: var(--font-size-sm); color: var(--text-muted); }

  .ff-foot {
    display: flex; align-items: center; gap: 6px;
    padding: 7px 12px; flex-shrink: 0;
    border-top: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
    font-size: var(--font-size-2xs); color: var(--text-disabled);
  }
  .ff-foot-open { display: inline-flex; align-items: center; gap: 4px; }
</style>
