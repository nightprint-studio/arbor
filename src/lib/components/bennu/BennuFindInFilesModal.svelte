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
   * Presentation and keyboard — the field, the windowed grouped list, the preview column,
   * ↑/↓/PageUp/PageDown/Enter — live in the shared `FindResults`. This modal keeps everything
   * that is Bennu's: the streaming buffer, the per-project prefs, the mask / module narrowing,
   * jar entries and the workspace scope. Enter opens the hit and closes, Esc cancels (Modal
   * owns Esc). Replace is intentionally out of scope (no affordance).
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
   *   * the list is **windowed** (inside `FindResults`), so five thousand matches are ~40 rows
   *     of DOM.
   *
   * The header row carries everything that decides **what is searched**: **whose text** (the
   * source picker — the project, its dependency jars, or both), and the two narrowings, the
   * **module** and the **file mask** (`*.java`, `*.jsp,*.tag`). The narrowings filter what the
   * scan returned rather than what it scans — the BE takes neither, and re-running the walk for
   * a test this side answers instantly would be slower AND less responsive — and both are
   * remembered per project.
   *
   * A hit inside a jar names its **artifact** and is **tinted**, because a result you can only
   * read is not the same kind of answer as one you can go and change.
   */
  import { Filter, FolderTree } from 'lucide-svelte';
  import BennuFileIcon from './BennuFileIcon.svelte';
  import { untrack } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import Modal from '$lib/components/shared/Modal.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import ToggleButton from '$lib/components/shared/ui/ToggleButton.svelte';
  import FindResults from '$lib/components/shared/ui/find/FindResults.svelte';
  import FindPreview from '$lib/components/shared/ui/find/FindPreview.svelte';
  import type { FindGroup, FindNotice, FindSegment } from '$lib/components/shared/ui/find/find-types';
  import FindFooter from '$lib/components/shared/internal/FindFooter.svelte';
  import { languageForPath } from './languages';
  import { moduleIndex } from './modules';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
  import { findInFiles, getFindPrefs, readFile, setFindPrefs, type FindSources } from '$lib/ipc/bennu';
  import { isJarEntry, openLibraryFile } from '$lib/ipc/bennu/library';
  import type { FindHit } from '$lib/types/bennu';

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
  /** The highlighted hit, as the flat index `FindResults` reports. */
  let sel = $state(0);

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

  // ── Groups: one per file, handed to the shared results widget ────────────────
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

  /**
   * The hits as `FindGroup`s — consecutive hits of one file share a header. The widget reports
   * the flat hit index, which is the position in `shown`, so `shown[i]` is always hit `i`'s
   * record. A group's `tooltip` is the hit's own file id: the header hover, and what the icon
   * is drawn from.
   */
  const groups = $derived.by<FindGroup[]>(() => {
    const out: FindGroup[] = [];
    let openFile: string | null = null;
    let group: FindGroup | null = null;
    shown.forEach((hit, idx) => {
      if (!group || hit.file !== openFile) {
        const jar = jarParts(hit.file);
        const rel = jar ? jar.entry : relPath(hit.file);
        const cut = rel.lastIndexOf('/');
        group = {
          key: `${out.length}:${hit.file}`,
          label: baseName(rel),
          detail: cut < 0 ? '' : rel.slice(0, cut),
          // Where the file is from — the module on a reactor, the artifact for a jar entry: the
          // answer to "is this mine" in the one place your eye already goes for the count.
          badge: jar ? jar.artifact : (modules.moduleOf(hit.file) ?? ''),
          tooltip: hit.file,
          external: !!jar,
          hits: [],
        };
        out.push(group);
        openFile = hit.file;
      }
      group.hits.push({
        key: `${hit.file}:${hit.line}:${hit.col}:${idx}`,
        location: `${hit.line}:${hit.col}`,
        text: hit.preview,
      });
    });
    for (const g of out) g.count = g.hits.length;
    return out;
  });

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

  function segments(text: string): FindSegment[] {
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

  const hasQuery = $derived(query.trim().length >= 2);

  // ── What the widget shows around the list ────────────────────────────────────
  /** Blocks the list: nothing to search in, or nothing asked yet. */
  const notice = $derived<FindNotice | null>(
    !projectStore.project
      ? { message: 'Open a project to search its files.' }
      : !hasQuery
        ? { message: 'Type at least 2 characters to search.' }
        : null,
  );

  const empty = $derived<FindNotice>(
    hits.length && narrowing
      ? { message: `${hits.length} match(es), none in ${narrowing}.` }
      : { message: errored ? 'Search is unavailable for this project.' : `No matches for “${query.trim()}”.` },
  );

  const fileCount = $derived(groups.length);
  const summary = $derived(
    hasQuery && shown.length
      ? `${shown.length} match${shown.length === 1 ? '' : 'es'} in ${fileCount} file${fileCount === 1 ? '' : 's'}`
      : '',
  );
  const summaryNote = $derived(hasQuery && shown.length && shown.length !== hits.length ? `of ${hits.length}` : '');

  // ── The preview column's body ────────────────────────────────────────────────
  const previewNote = $derived<string | null>(
    previewError ? 'This file can’t be previewed.' : previewText && current ? null : 'Reading…',
  );
  const previewLanguage = $derived(previewFile ? languageForPath(previewFile) : undefined);
</script>

<!-- No title bar, like the go-to modals: the field is focused the moment it opens and its
     placeholder already says what this is. A chrome row whose only job is to repeat that costs
     a row of the results. -->
<Modal {onClose} width="1000px" height="620px" padBody={false} ariaLabel="Find in project">
  <FindResults
    bind:query
    bind:selected={sel}
    {groups}
    {loading}
    {capped}
    {notice}
    {empty}
    {summary}
    {summaryNote}
    placeholder="Find in project…"
    ariaLabel="Find in project"
    segmentsOf={(h) => segments(h.text)}
    onOpen={(i) => { const h = shown[i]; if (h) void openHit(h); }}
    {controls}
    {toggles}
    {groupLead}
    {preview}
    {footer}
  />
</Modal>

<!--
  The header row, the go-to modals' one: everything that decides WHAT IS SEARCHED, and nothing
  that decides how. The source picker says whose text, the module and the mask narrow what comes
  back, and the count beside them is the answer to all three. None of it belongs beside the
  field, which is for the query and for how the query is read.
-->
{#snippet controls()}
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
{/snippet}

<!-- The keys of a search bar: how a match is judged, and how far out of this project it looks.
     All of them are one bit each and all re-run the search on the spot, which is what a pressed
     key says and a dropdown does not. -->
{#snippet toggles()}
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
{/snippet}

<!-- The tree's own icon rule, not a stand-in for it: a `.java` wears the kind it declares and
     everything else its file type, exactly as in the project tree and the go-to modals. -->
{#snippet groupLead(group: FindGroup)}
  <BennuFileIcon size={13} path={group.tooltip ?? group.label} />
{/snippet}

<!-- The real editor, so the context is coloured exactly as the buffer is — the same grammar,
     not a second highlighter that agrees with it on Java and not on XML. -->
{#snippet preview()}
  <FindPreview
    label={previewNote ? '' : previewLabel}
    labelTooltip={previewFile}
    note={previewNote}
    text={previewText}
    language={previewLanguage}
    activeLine={current?.line}
  />
{/snippet}

{#snippet footer()}
  <FindFooter />
{/snippet}

<style>
  /* Sized like the module picker beside it: wide enough for a couple of globs, never wide enough
     to compete with the query field below. */
  .ff-mask { display: flex; align-items: center; gap: 6px; min-width: 0; width: 190px; flex-shrink: 0; }
  .ff-mask :global(svg) { color: var(--text-disabled); flex-shrink: 0; }
</style>
