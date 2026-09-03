<script lang="ts">
  /**
   * Bennu settings — the two-pane settings surface (shared `SettingsShell`), the
   * same look as Arbor's and merula's settings.
   *
   * Editable groups (Editor / Completion / Folding / Java) are backed by the
   * `bennuSettingsStore` rune store — apply-on-change, so there's no Save button:
   * Esc / Done just closes. Persistence is MOCK (in-memory) today; the store is
   * the seam for a future typed `[bennu]` config (rule 11).
   *
   * The Project group stays read-only: it reflects the resolved facts about the
   * open project — the JDK, the detected capabilities and their evidence, and the
   * active file's encoding.
   *
   * The first group is not Bennu's at all: `Interface → Appearance` is the shell's shared
   * settings (font scale, window controls, the compact title bar), which already applied to
   * this window and simply had no dialog in it. It is the same component Corvus's settings
   * panel renders.
   */
  import { tooltip } from '$lib/actions/tooltip';
  import {
    Settings, Coffee, Boxes, FileType, TextCursorInput, ListTree, Bug,
    FoldVertical, Braces, RotateCcw, Wand2, Plus, Trash2, TriangleAlert, FolderOpen,
    Database, Package, ServerCog, RefreshCw, CircleCheck, Download, Monitor,
    Sparkles, Command, Terminal,
  } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import SettingsShell, { type SettingsNavGroup } from '$lib/components/shared/ui/SettingsShell.svelte';
  import AppearanceSettings from '$lib/components/shared/internal/AppearanceSettings.svelte';
  import AnimationsSettings from '$lib/components/shared/internal/AnimationsSettings.svelte';
  import KeystrokesSettings from '$lib/components/shared/internal/KeystrokesSettings.svelte';
  import TerminalsSettings from '$lib/components/shared/internal/TerminalsSettings.svelte';
  import ThemeEditorModal from '$lib/components/shared/ThemeEditorModal.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import {
    bennuSettingsStore, SOURCE_ENCODINGS, SQL_DIALECTS,
    type IndentStyle, type SourceEncoding, type SqlDialectSetting,
  } from '$lib/stores/bennu/settings.svelte';
  import { bennuDiagnosticsStore } from '$lib/stores/bennu/diagnostics.svelte';
  import { bennuLspStore } from '$lib/stores/bennu/lsp.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import {
    getBennuConfig, setBennuConfig, type BennuConfig, type CargoConfigDto, type LspConfigDto,
  } from '$lib/ipc/bennu/config';
  import { getStepExcludes } from '$lib/ipc/bennu/debug';

  let { onClose }: { onClose: () => void } = $props();

  /** The theme editor, opened from the Appearance page. Mounted here as well as on the title
   *  bar's gear because a settings page that names the theme and cannot change it sends you
   *  looking through the chrome for the button — the modal itself is stateless and shared. */
  let themeEditorOpen = $state(false);

  // ── JDK search paths (real bennu config, not the mock settings store) ─────────
  // Loaded once on mount; edits persist through `set_bennu_config`, which also re-seeds
  // the backend's classpath search so the change applies on the next index build.
  let cfg = $state<BennuConfig | null>(null);
  let jdkPickerOpen = $state(false);
  $effect(() => { void getBennuConfig().then((c) => { cfg = c; }).catch(() => {}); });

  const jdkPaths = $derived(cfg?.jdk_paths ?? []);
  const jdkReport = $derived(bennuDiagnosticsStore.jdk);

  /** Persist a partial config change with a fresh read-modify-write, so a field another writer owns
   *  (autosave / auto-import via the settings store, build type via the run store) is never clobbered
   *  by this modal's stale snapshot. Updates the local `cfg` to the written result. */
  async function saveConfigPatch(patch: Partial<BennuConfig>) {
    const cur = await getBennuConfig().catch(() => null);
    if (!cur) return;
    cfg = { ...cur, ...patch };
    await setBennuConfig(cfg).catch(() => {});
  }

  // ── Debugger: what a step passes straight through ────────────────────────────
  /** The patterns actually in force, from the backend: the configured list, or its defaults.
   *  Never assembled here — a copy of the default list on this side would be a second answer
   *  that drifts from the one doing the stepping. */
  let stepExcludes = $state<string[]>([]);
  /** Whether the list on screen is the backend's default rather than one of your own — what
   *  makes "Reset" meaningful and what the hint says. */
  const usingDefaults = $derived((cfg?.step_excludes ?? []).length === 0);
  let excludeDraft = $state('');

  async function loadExcludes() {
    stepExcludes = await getStepExcludes().catch(() => []);
  }
  $effect(() => { void loadExcludes(); });

  /** Whether the VM will accept this pattern: a `*` at one end, or none. One bad entry makes
   *  the VM refuse the whole step request, which shows up as stepping having quietly stopped
   *  working — so it is refused here, where there is somewhere to say so. */
  function validPattern(p: string): boolean {
    const inner = p.trim().replace(/^\*/, '').replace(/\*$/, '');
    return inner.length > 0 && !inner.includes('*');
  }
  const draftValid = $derived(!excludeDraft.trim() || validPattern(excludeDraft));

  /** Persist a new list and re-read what the backend made of it. Writing the *effective* list
   *  is what turns the defaults into a list of your own — adding one pattern to an empty
   *  config would otherwise mean stepping into the JDK from then on. */
  async function commitExcludes(next: string[]) {
    await saveConfigPatch({ step_excludes: next });
    await loadExcludes();
  }
  async function addExclude() {
    const p = excludeDraft.trim();
    if (!p || !validPattern(p) || stepExcludes.includes(p)) return;
    excludeDraft = '';
    await commitExcludes([...stepExcludes, p]);
  }
  async function removeExclude(p: string) {
    await commitExcludes(stepExcludes.filter((x) => x !== p));
  }
  /** Back to the backend's list — an empty array, which is what "use the defaults" means. */
  async function resetExcludes() {
    await commitExcludes([]);
  }

  async function commitJdkPaths(paths: string[]) {
    await saveConfigPatch({ jdk_paths: paths });
    // Re-fetch the JDK status so the titlebar / Problems / this card reflect the new paths.
    const root = projectStore.project?.root;
    if (root) void bennuDiagnosticsStore.refresh(root);
  }
  function onPickJdk(dir: string) {
    jdkPickerOpen = false;
    const paths = [...(cfg?.jdk_paths ?? [])];
    if (!paths.includes(dir)) void commitJdkPaths([...paths, dir]);
  }
  function removeJdkPath(p: string) {
    void commitJdkPaths((cfg?.jdk_paths ?? []).filter((x) => x !== p));
  }

  // ── Library beans: which dependencies are read ────────────────────────────────
  // Four axes, each a list, edited as comma-separated text: the entries are coordinates
  // people paste from a pom, and a chip editor would make pasting four of them slower than
  // typing them. Committed on blur/Enter so a half-typed prefix never triggers a scan.
  const beanAxes = [
    { key: 'group_id',           label: 'Group ids',           hint: 'com.acme.platform' },
    { key: 'group_id_prefix',    label: 'Group id prefixes',   hint: 'com.acme.  — the trailing dot matters' },
    { key: 'artifact_id',        label: 'Artifact ids',        hint: 'shared-security' },
    { key: 'artifact_id_prefix', label: 'Artifact id prefixes', hint: 'acme-starter-' },
  ] as const;

  const libraryBeans = $derived(
    cfg?.library_beans ?? { group_id: [], group_id_prefix: [], artifact_id: [], artifact_id_prefix: [] },
  );
  const beansAllowlistEmpty = $derived(
    beanAxes.every((a) => (libraryBeans[a.key] ?? []).length === 0),
  );

  /** Split a comma/newline-separated field into entries, dropping blanks — an empty prefix
   *  would otherwise mean "every artifact", which is not a reasonable reading of a stray
   *  comma. (The backend refuses an empty prefix too; this keeps the file tidy.) */
  function parseAxis(text: string): string[] {
    return text.split(/[,\n]/).map((s) => s.trim()).filter(Boolean);
  }

  async function commitBeanAxis(key: (typeof beanAxes)[number]['key'], text: string) {
    await saveConfigPatch({ library_beans: { ...libraryBeans, [key]: parseAxis(text) } });
  }

  // ── Validate-project-on-open (real bennu config, default on) ──────────────────
  const validateOnOpen = $derived(cfg?.validate_on_open ?? true);
  async function commitValidateOnOpen(v: boolean) {
    await saveConfigPatch({ validate_on_open: v });
  }

  // ── Validation CPU threads (0 = auto ≈ half the cores) ────────────────────────
  const validationThreads = $derived(cfg?.validation_threads ?? 0);
  async function commitValidationThreads(v: string) {
    const n = Math.max(0, Math.floor(Number(v) || 0));
    await saveConfigPatch({ validation_threads: n });
  }

  // ── Indexing CPU threads (1 = serial, the default; 0 = auto) ──────────────────
  const indexThreads = $derived(cfg?.index_threads ?? 1);
  async function commitIndexThreads(v: string) {
    const n = Math.max(0, Math.floor(Number(v) || 0));
    await saveConfigPatch({ index_threads: n });
  }

  /** The Java-only sections drop out on a Cargo project: JDK, Capabilities and the
   *  Java/Java-Style pages are each a statement about a Java stack, and the encoding page
   *  has nothing to resolve (Rust is UTF-8 by definition). Editor / Completion / Folding
   *  apply to every buffer and stay. */
  const groups = $derived<SettingsNavGroup[]>([
    // First, and above the editor: it is the group that answers "why is everything so small",
    // which is a question asked before any of the others. The settings under it are the shell's
    // and apply to every Arbor window — Bennu is simply one of the places you can reach them.
    { label: 'Interface', items: [
      { id: 'appearance', label: 'Appearance', icon: Monitor },
      { id: 'animations', label: 'Animations', icon: Sparkles },
      { id: 'keystrokes', label: 'Keyboard Inputs', icon: Command },
      // The shells the built-in Terminal offers. Bennu's terminal is the same one Arbor's is
      // — same store, same backend — so this is not a copy of that page, it is that page.
      { id: 'terminals',  label: 'Terminals', icon: Terminal },
    ] },
    { label: 'Editor', items: [
      { id: 'editor',     label: 'Editor',     icon: TextCursorInput },
      { id: 'completion', label: 'Completion', icon: ListTree },
      { id: 'folding',    label: 'Folding',    icon: FoldVertical },
      // Always present, on every project kind: it is where a *missing* server is explained,
      // and hiding it on a Java project would hide the answer to "why does my `.rs` file have
      // no go-to" from exactly the polyglot repo that has one.
      { id: 'languages',  label: 'Language Servers', icon: ServerCog },
      ...(projectStore.isCargo ? [] : [
        { id: 'style',    label: 'Java Style', icon: Wand2 },
        { id: 'java',     label: 'Java',       icon: Braces },
        { id: 'debugger', label: 'Debugger',   icon: Bug },
      ]),
    ] },
    ...(projectStore.isCargo ? [] : [{
      label: 'Project', items: [
        { id: 'jdk',          label: 'JDK',          icon: Coffee },
        { id: 'capabilities', label: 'Capabilities', icon: Boxes },
        { id: 'encoding',     label: 'Encoding',     icon: FileType },
      ],
    }, {
      label: 'Spring', items: [
        { id: 'beans', label: 'Beans', icon: Boxes },
      ],
    }]),
  ]);
  let active = $state('editor');

  // Something opened Settings *for a reason* (the status bar's "server not running" pill) and
  // asked for a page. Honoured once, then cleared, so ordinary re-opens stay where the user was.
  $effect(() => {
    const requested = bennuUiStore.settingsSection;
    if (!requested) return;
    active = requested;
    bennuUiStore.consumeSettingsSection();
  });

  // A section that just disappeared (project switched to Cargo while it was open) would
  // leave the shell on a page with no nav entry. Fall back to Editor.
  $effect(() => {
    const ids = groups.flatMap((g) => g.items.map((i) => i.id));
    if (!ids.includes(active)) active = 'editor';
  });

  // ── Language servers ──────────────────────────────────────────────────────────
  //
  // Two lists, because they answer different questions: what Bennu *can* run on this machine
  // (with an install hint for each one it cannot find), and what is running *right now* for the
  // open projects. A settings page that showed only the first would never explain a server that
  // is installed and crashing.
  /** The `[lsp]` section, with a complete default so a config written before it existed reads as
   *  "on, nothing disabled" rather than as a half-populated object. */
  const lspCfg = $derived<LspConfigDto>(
    cfg?.lsp ?? {
      enabled: true,
      rust_check_command: 'check',
      disabled: [],
      server_paths: {},
      servers: [],
      background_idle_timeout_secs: 600,
    },
  );
  const lspEnabled = $derived(lspCfg.enabled);
  const lspDisabled = $derived(lspCfg.disabled ?? []);
  const serverPaths = $derived(lspCfg.server_paths ?? {});
  const rustCheckCommand = $derived(lspCfg.rust_check_command || 'check');
  // `?? 600` and not `|| 600`: zero is a real choice here — never reclaim — and `||` would read it
  // as absent and silently put ten minutes back.
  const backgroundIdle = $derived(String(lspCfg.background_idle_timeout_secs ?? 600));

  /** Whether a Rust server exists to configure at all — the check command is rust-analyzer's, and a
   *  setting for a server this machine has never had is a setting for nothing. */
  const hasRustServer = $derived(bennuLspStore.servers.some((x) => x.language === 'rust'));

  /** The `[cargo]` section, with a complete default so a config written before it existed reads as
   *  "on, a day" rather than as a half-populated object. */
  const cargoCfg = $derived<CargoConfigDto>(
    cfg?.cargo ?? { crates_io: true, index_ttl_hours: 24 },
  );
  const cratesIo = $derived(cargoCfg.crates_io);

  async function setCratesIo(on: boolean) {
    await saveConfigPatch({ cargo: { ...cargoCfg, crates_io: on } });
  }

  async function setRustCheckCommand(command: string) {
    await saveConfigPatch({ lsp: { ...lspCfg, rust_check_command: command } });
    // It is an `initializationOptions` value, so it only takes effect on a fresh handshake. Saying
    // so beats a setting that appears to do nothing until the next time the app happens to restart.
    await bennuLspStore.reloadServers();
  }

  async function setBackgroundIdle(secs: string) {
    await saveConfigPatch({ lsp: { ...lspCfg, background_idle_timeout_secs: Number(secs) } });
    // No server restart: the reaper reads this on every pass, so a change applies to the sessions
    // already running rather than only to the next ones.
  }

  async function setLspEnabled(on: boolean) {
    await saveConfigPatch({ lsp: { ...lspCfg, enabled: on } });
    await bennuLspStore.reloadServers();
  }

  async function toggleServer(id: string, on: boolean) {
    const next = on ? lspDisabled.filter((d) => d !== id) : [...new Set([...lspDisabled, id])];
    await saveConfigPatch({ lsp: { ...lspCfg, disabled: next } });
    await bennuLspStore.reloadServers();
  }

  /**
   * Install a language server from its own package manager, streaming into the Build panel.
   *
   * The toast is where the outcome lands rather than an inline banner: the interesting part
   * of a `cargo install --git` is the three minutes of log, which is already on screen in the
   * panel, and what is left to say afterwards is one sentence.
   */
  async function installServer(id: string) {
    const res = await bennuLspStore.install(id);
    toastStore.show(res.message, res.ok ? 'success' : 'error', res.ok ? 5000 : 9000);
  }

  async function commitServerPath(id: string, path: string) {
    const next = { ...serverPaths };
    const trimmed = path.trim();
    if (trimmed) next[id] = trimmed;
    else delete next[id];
    await saveConfigPatch({ lsp: { ...lspCfg, server_paths: next } });
    await bennuLspStore.reloadServers();
  }

  const s = bennuSettingsStore;

  const indentOptions = [
    { value: 'spaces', label: 'Spaces' },
    { value: 'tabs',   label: 'Tabs' },
  ];
  const encodingOptions = SOURCE_ENCODINGS.map((e) => ({ value: e, label: e }));
  /** Engine labels, not the raw setting values — "postgres" in a dropdown reads like a
   *  hostname. `portable` leads: it is the default and the safe answer. */
  const SQL_DIALECT_LABELS: Record<SqlDialectSetting, string> = {
    portable: 'Portable (both engines)',
    oracle: 'Oracle / PL-SQL',
    postgres: 'PostgreSQL',
  };
  const sqlDialectOptions = SQL_DIALECTS.map((d) => ({ value: d, label: SQL_DIALECT_LABELS[d] }));

  // ── Project (read-only facts) ─────────────────────────────────────────────
  const project = $derived(projectStore.project);
  const jdk = $derived(project?.jdk ?? null);
  const caps = $derived(projectStore.capabilities);
  // Gates the "Lombok val" style toggle — off when Lombok isn't on the classpath.
  const hasLombok = $derived(caps?.lombok ?? false);

  // Enabled capability field names (skip the `hits` array).
  const enabledCaps = $derived.by(() => {
    if (!caps) return [] as string[];
    return Object.entries(caps)
      .filter(([k, v]) => k !== 'hits' && v === true)
      .map(([k]) => k);
  });
  const hits = $derived(caps?.hits ?? []);
  function capLabel(field: string): string {
    return field.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
  }

  // ── Live preview snippets ─────────────────────────────────────────────────
  // Tiny read-only examples that mirror the current settings so a toggle's effect
  // is visible before it touches real code. Derived → they re-render on any change.

  // One indent unit: real tabs when the indent style is tabs, else N spaces. Used
  // by both previews so their indentation tracks the Editor settings faithfully.
  const indentUnit = $derived(s.indentStyle === 'tabs' ? '\t' : ' '.repeat(s.tabSize));

  // Java Style preview — a minimal class whose formatting reflects the Style card:
  // final params, Lombok val locals, arrow/return switch, brace spacing, and the
  // blank line between members.
  const styleSnippet = $derived.by(() => {
    const i = indentUnit;
    const fin = s.finalParams ? 'final ' : '';
    const localDecl = s.useLombokVal ? 'val' : 'String';
    // Spaces-in-braces only affects single-line bodies (the getter here).
    const openB = s.spaceInBraces ? '{ ' : '{';
    const closeB = s.spaceInBraces ? ' }' : '}';
    const gap = s.blankLineBetweenMembers ? '\n' : '';

    const getter = `${i}public String getName() ${openB}return this.name;${closeB}`;

    const label = s.switchWithReturn
      ? [
          `${i}public String label(${fin}int code) {`,
          `${i}${i}return switch (code) {`,
          `${i}${i}${i}case 0 -> "off";`,
          `${i}${i}${i}default -> "on";`,
          `${i}${i}};`,
          `${i}}`,
        ].join('\n')
      : [
          `${i}public String label(${fin}int code) {`,
          `${i}${i}${localDecl} result;`,
          `${i}${i}switch (code) {`,
          `${i}${i}${i}case 0: result = "off"; break;`,
          `${i}${i}${i}default: result = "on";`,
          `${i}${i}}`,
          `${i}${i}return result;`,
          `${i}}`,
        ].join('\n');

    return [
      'public class Account {',
      `${i}private String name;`,
      gap ? '' : null,
      getter,
      gap ? '' : null,
      label,
      '}',
    ].filter((l) => l !== null).join('\n');
  });

  // Editor preview — a short block whose indentation, whitespace glyphs and margin
  // guide mirror the Editor card (tabSize / indentStyle / showWhitespace / rightMargin).
  const editorSnippet = $derived.by(() => {
    const i = indentUnit;
    const raw = [
      'void demo() {',
      `${i}int total = compute();`,
      `${i}${i}// nested`,
      '}',
    ].join('\n');
    // showWhitespace → render the same dot/arrow glyphs the editor would (spaces → ·,
    // tabs → →). Kept preview-only so the real source text is never mutated.
    if (!s.showWhitespace) return raw;
    return raw.replace(/\t/g, '→').replace(/ /g, '·');
  });

  // Column ruler for the Editor preview — mirrors the right-margin guide (0 hides it).
  const editorRuler = $derived(s.rightMargin > 0);
</script>

<Modal {onClose} width="840px" height="560px" padBody={false} ariaLabel="Bennu Settings">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Settings size={14} />
      <span class="modal-title">Settings</span>
    </ModalHeader>
  {/snippet}

  <SettingsShell {groups} bind:active>
    {#snippet content()}
      {#if active === 'appearance'}
        <AppearanceSettings
          onOpenThemeEditor={() => { themeEditorOpen = true; }}
          onCustomizeBars={() => { onClose(); bennuUiStore.openCustomizeRails(); }}
        />
      {:else if active === 'animations'}
        <AnimationsSettings />
      {:else if active === 'keystrokes'}
        <KeystrokesSettings />
      {:else if active === 'terminals'}
        <TerminalsSettings />
      {:else if active === 'editor'}
        <div class="section-header">
          <h2>Editor</h2>
          <p>How source is rendered and how the Tab key indents.</p>
        </div>
        <div class="card">
          <div class="card-section-title"><TextCursorInput size={12} /> Appearance</div>
          <FormRow label="Font size" description="Point size of the editor's monospaced text.">
            <NumberStepper value={s.fontSize} min={8} max={32} narrow suffix="px"
                           onchange={(v) => s.setFontSize(v)} ariaLabel="Editor font size" />
          </FormRow>
          <FormRow label="Autosave" description="Write a modified file to disk automatically — after a short idle, when you switch tabs, and when the window loses focus. Off saves only on Ctrl+S.">
            <Toggle checked={s.autosave} onchange={(v) => s.setAutosave(v)} ariaLabel="Autosave" />
          </FormRow>
          <FormRow label="Local history" description="Keep a private record of what every project file used to be, so a save, a refactor or a delete can be undone long after the editor's own undo has moved on. Stored in Arbor's data folder, never inside the project. Files git ignores are skipped.">
            <Toggle checked={s.localHistory} onchange={(v) => s.setLocalHistory(v)} ariaLabel="Local history" />
          </FormRow>
          {#if s.localHistory}
            <FormRow label="Keep history for" description="Labelled revisions, and each file's newest one, are kept regardless — a label is a promise, and a file whose only revision aged out would stop having a history exactly when it is the last copy.">
              <NumberStepper value={s.localHistoryDays} min={1} max={90} narrow suffix="days"
                             onchange={(v) => s.setLocalHistoryDays(v)} ariaLabel="Days of local history" />
            </FormRow>
            <FormRow label="History size limit" description="Per project. Over it, the oldest revisions go first.">
              <NumberStepper value={s.localHistoryMaxMb} min={16} max={4096} step={16} narrow suffix="MB"
                             onchange={(v) => s.setLocalHistoryMaxMb(v)} ariaLabel="Local history size limit" />
            </FormRow>
            <FormRow label="Skip files larger than" description="One large binary would spend the whole budget on a single revision that no diff can show anyway.">
              <NumberStepper value={s.localHistoryMaxFileMb} min={1} max={128} narrow suffix="MB"
                             onchange={(v) => s.setLocalHistoryMaxFileMb(v)} ariaLabel="Local history file size ceiling" />
            </FormRow>
          {/if}
          <FormRow label="Show line numbers" description="Gutter line numbers on the left margin.">
            <Toggle checked={s.showLineNumbers} onchange={(v) => s.setShowLineNumbers(v)} ariaLabel="Show line numbers" />
          </FormRow>
          <FormRow label="Highlight current line" description="Tint the line the caret sits on.">
            <Toggle checked={s.highlightCurrentLine} onchange={(v) => s.setHighlightCurrentLine(v)} ariaLabel="Highlight current line" />
          </FormRow>
          <FormRow label="Scrollbar overview" description="Mark errors/warnings on the right-edge strip and preview the file on hover (replaces the scrollbar). Applies on the next file opened.">
            <Toggle checked={s.minimap} onchange={(v) => s.setMinimap(v)} ariaLabel="Scrollbar overview" />
          </FormRow>
          <FormRow label="Indentation guides" description="Faint vertical lines per indent level; the block the caret is in is brightened. Applies on the next file opened.">
            <Toggle checked={s.indentGuides} onchange={(v) => s.setIndentGuides(v)} ariaLabel="Indentation guides" />
          </FormRow>
          <FormRow label="Sticky scroll" description="Pin the enclosing class and method signatures to the top while scrolling. Applies on the next file opened.">
            <Toggle checked={s.stickyScroll} onchange={(v) => s.setStickyScroll(v)} ariaLabel="Sticky scroll" />
          </FormRow>
          <FormRow label="Inlay hints" description="Draw the parameter name in front of each argument that doesn't already say what it is, and the type a `var` was inferred as. Not part of the file — they can't be selected or copied.">
            <Toggle checked={s.inlayHints} onchange={(v) => s.setInlayHints(v)} ariaLabel="Inlay hints" />
          </FormRow>
          <FormRow label="Word wrap" description="Wrap long lines to the viewport instead of scrolling horizontally.">
            <Toggle checked={s.wordWrap} onchange={(v) => s.setWordWrap(v)} ariaLabel="Word wrap" />
          </FormRow>
          <FormRow label="Show whitespace" description="Render dots and arrows for spaces and tabs.">
            <Toggle checked={s.showWhitespace} onchange={(v) => s.setShowWhitespace(v)} ariaLabel="Show whitespace" />
          </FormRow>
          <FormRow label="Right margin" description="Column for the vertical margin guide (0 hides it). Applies on the next file opened.">
            <NumberStepper value={s.rightMargin} min={0} max={240} step={10} narrow suffix="col"
                           onchange={(v) => s.setRightMargin(v)} ariaLabel="Right margin column" />
          </FormRow>
        </div>
        <div class="card">
          <div class="card-section-title"><TextCursorInput size={12} /> Indentation</div>
          <FormRow label="Tab size" description="Number of columns a tab occupies.">
            <NumberStepper value={s.tabSize} min={1} max={16} narrow
                           onchange={(v) => s.setTabSize(v)} ariaLabel="Tab size" />
          </FormRow>
          <FormRow label="Indent using" description="Insert spaces, or keep hard tab characters.">
            <RadioGroup value={s.indentStyle} options={indentOptions} size="sm"
                        onchange={(v) => s.setIndentStyle(v as IndentStyle)} />
          </FormRow>
        </div>
        <div class="card">
          <div class="card-section-title"><Database size={12} /> SQL</div>
          <FormRow
            label="Dialect"
            description="Which engine’s rules colour a .sql file. Nothing in the file says which one it targets, and the two disagree about string quoting — Portable uses the rules valid on both. Applies on the next file opened."
          >
            <Select value={s.sqlDialect} options={sqlDialectOptions}
                    onchange={(v) => s.setSqlDialect(v as SqlDialectSetting)} ariaLabel="SQL dialect" />
          </FormRow>
        </div>
        <div class="card">
          <div class="card-section-title"><TextCursorInput size={12} /> Preview</div>
          <div class="bs-snippet-wrap">
            {#if editorRuler}
              <!-- Margin guide: a thin rule at the configured column, tabSize-relative -->
              <span class="bs-snippet-ruler" style="left: calc({s.rightMargin}ch + 12px);" aria-hidden="true"></span>
            {/if}
            <pre class="bs-snippet" style="tab-size: {s.tabSize};" aria-label="Editor preview">{editorSnippet}</pre>
          </div>
        </div>

      {:else if active === 'completion'}
        <div class="section-header">
          <h2>Completion</h2>
          <p>When the completion popup appears and how it matches what you type.</p>
        </div>
        <div class="card">
          <div class="card-section-title"><ListTree size={12} /> Popup</div>
          <FormRow label="Auto-popup on typing" description="Show suggestions automatically as you type an identifier.">
            <Toggle checked={s.autoPopup} onchange={(v) => s.setAutoPopup(v)} ariaLabel="Auto-popup on typing" />
          </FormRow>
          <FormRow label="Popup delay" description="How long to wait before the auto-popup opens.">
            <NumberStepper value={s.popupDelayMs} min={0} max={2000} step={50} narrow suffix="ms"
                           disabled={!s.autoPopup}
                           onchange={(v) => s.setPopupDelayMs(v)} ariaLabel="Popup delay in milliseconds" />
          </FormRow>
          <FormRow label="Case-sensitive matching" description="Require the prefix's case to match the candidate.">
            <Toggle checked={s.caseSensitive} onchange={(v) => s.setCaseSensitive(v)} ariaLabel="Case-sensitive matching" />
          </FormRow>
          <FormRow label="Auto-import on accept" description="Add the missing import when you accept a completion.">
            <Toggle checked={s.autoImport} onchange={(v) => s.setAutoImport(v)} ariaLabel="Auto-import on accept" />
          </FormRow>
        </div>

      {:else if active === 'folding'}
        <div class="section-header">
          <h2>Folding</h2>
          <p>Code-folding behaviour in the editor gutter.</p>
        </div>
        <div class="card">
          <div class="card-section-title"><FoldVertical size={12} /> Regions</div>
          <FormRow label="Enable code folding" description="Show fold controls for methods, classes and blocks.">
            <Toggle checked={s.foldingEnabled} onchange={(v) => s.setFoldingEnabled(v)} ariaLabel="Enable code folding" />
          </FormRow>
          <FormRow label="Fold block comments by default" description="Collapse /* … */ comments when a file opens.">
            <Toggle checked={s.foldBlockComments} disabled={!s.foldingEnabled}
                    onchange={(v) => s.setFoldBlockComments(v)} ariaLabel="Fold block comments by default" />
          </FormRow>
        </div>

      {:else if active === 'languages'}
        <div class="section-header">
          <h2>Language Servers</h2>
          <p>
            Bennu's Java intelligence is its own engine. Every other language — Rust first — is
            served by an external <strong>language server</strong>: it supplies completion, go-to,
            find-usages, diagnostics, rename, formatting and the semantic colouring.
          </p>
        </div>

        <div class="card">
          <div class="card-section-title"><ServerCog size={12} /> General</div>
          <FormRow
            label="Enable language servers"
            description="A server only starts for a project whose root carries the matching manifest (a Cargo.toml for Rust) and whose binary is installed — so leaving this on costs nothing when neither is true."
          >
            <Toggle checked={lspEnabled} onchange={(v) => void setLspEnabled(v)}
                    ariaLabel="Enable language servers" />
          </FormRow>
          <!-- Only meaningful while servers can start at all. Not under Rust: it is about every
               language server, and rust-analyzer is merely the one that costs the most. -->
          {#if lspEnabled}
            <FormRow
              label="Stop unattended servers after"
              description="A language server can be started by an AI client asking about a project you do not have open. Nothing on screen is using it, and rust-analyzer holds most of a gigabyte, so it is stopped once it goes quiet — this is how long that takes. A server for a project you have open is never stopped. Applies to servers already running."
            >
              <Select
                value={backgroundIdle}
                options={[
                  { value: '300', label: '5 minutes' },
                  { value: '600', label: '10 minutes' },
                  { value: '1800', label: '30 minutes' },
                  { value: '3600', label: '1 hour' },
                  { value: '0', label: 'Never — keep them running' },
                ]}
                onchange={(v) => void setBackgroundIdle(v)}
              />
            </FormRow>
          {/if}
        </div>

        <!-- Rust's own knob. Only shown when there is a Rust server to configure: a setting for a
             server this machine has never had is a setting for nothing. -->
        {#if hasRustServer}
          <div class="card">
            <div class="card-section-title"><ServerCog size={12} /> Rust</div>
            <FormRow
              label="Diagnostics on save"
              description="What rust-analyzer runs after each save to produce the compiler's real diagnostics — types and borrows, as opposed to the syntactic ones the parser alone can see. Clippy is a superset: every cargo check error plus several hundred lints, at the cost of a slower build after every save. Takes effect on the next server start."
            >
              <Select
                value={rustCheckCommand}
                options={[
                  { value: 'check', label: 'cargo check — faster' },
                  { value: 'clippy', label: 'cargo clippy — also the lints' },
                ]}
                onchange={(v) => void setRustCheckCommand(v)}
              />
            </FormRow>
            <!-- The one place Bennu reaches the network by itself, so it says so plainly and can be
                 turned off. Beside the check command because both are "how Rust tooling behaves",
                 and because this is where someone looks for it. -->
            <FormRow
              label="Check crates.io for newer versions"
              description="Reads the crates.io index to mark a dependency in Cargo.toml that is behind, and to offer a version list when adding one. Answers come from a cache on disk, refreshed at most once a day per crate. Off keeps Bennu entirely local — adding a dependency still works, cargo just picks the version."
            >
              <Toggle
                checked={cratesIo}
                onchange={(on) => void setCratesIo(on)}
                ariaLabel="Query the crates.io index"
              />
            </FormRow>
          </div>
        {/if}

        <!-- What is running RIGHT NOW. Separate from the catalogue below because a server can be
             installed and still be failing, and only this list can say so. -->
        {#if bennuLspStore.statuses.length}
          <div class="card">
            <div class="card-section-title"><RefreshCw size={12} /> Running</div>
            {#each bennuLspStore.statuses as st (st.root + st.language)}
              <div class="lsp-run">
                <div class="lsp-run-main">
                  <div class="lsp-run-head">
                    <span class="lsp-name">{st.version ?? st.name}</span>
                    <Badge
                      variant="tone"
                      tone={st.state === 'ready' ? 'success'
                        : st.state === 'starting' ? 'info' : 'error'}
                    >{st.state}</Badge>
                    {#if st.progress}<span class="lsp-progress">{st.progress}</span>{/if}
                  </div>
                  <div class="lsp-run-sub">{st.root}</div>
                  {#if st.message}
                    <div class="lsp-run-msg">{st.message}</div>
                  {/if}
                  {#if st.state !== 'ready' && st.log_tail.length}
                    <!-- The server's own stderr. Usually the only place a refusal to start
                         explains itself, so it is shown rather than kept in a log nobody opens. -->
                    <pre class="lsp-log">{st.log_tail.slice(-8).join('\n')}</pre>
                  {/if}
                </div>
                <div class="lsp-run-actions">
                  <Button
                    size="sm" variant="ghost"
                    onclick={() => void bennuLspStore.restart(st.root, st.language)}
                  >Restart</Button>
                  {#if st.state === 'ready'}
                    <Button
                      size="sm" variant="ghost"
                      onclick={() => void bennuLspStore.stop(st.root, st.language)}
                    >Stop</Button>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        {/if}

        <div class="card">
          <div class="card-section-title"><Package size={12} /> Installed servers</div>
          {#if !bennuLspStore.servers.length}
            <EmptyState
              message="No servers in the catalogue"
              description="Bennu could not read the language-server list from the backend."
            />
          {:else}
            {#each bennuLspStore.servers as srv (srv.id)}
              <div class="lsp-srv">
                <div class="lsp-srv-head">
                  {#if srv.path}
                    <CircleCheck size={13} class="lsp-ok" />
                  {:else}
                    <TriangleAlert size={13} class="lsp-warn" />
                  {/if}
                  <span class="lsp-name">{srv.name}</span>
                  <span class="lsp-exts">{srv.extensions.map((e) => `.${e}`).join(' ')}</span>
                  {#if srv.custom}<Badge variant="tone" tone="info">custom</Badge>{/if}
                  <span class="lsp-spacer"></span>
                  <Toggle
                    checked={srv.enabled}
                    disabled={!lspEnabled}
                    onchange={(v) => void toggleServer(srv.id, v)}
                    ariaLabel={`Enable ${srv.name}`}
                  />
                </div>
                {#if srv.path}
                  <div class="lsp-srv-path" use:tooltip={srv.path}>{srv.path}</div>
                {:else}
                  <!-- Not "not found" full stop: the hint is what turns a dead end into a next
                       step, which is the whole reason the catalogue carries one. The hint says
                       where the server comes from; the command below is what to run, shown
                       whether or not there is a button — a user who prefers their own terminal
                       needs it, and a failed install leaves them with exactly that. -->
                  <div class="lsp-srv-hint">
                    <code>{srv.command}</code> was not found. {srv.install_hint}
                  </div>
                  {#if srv.install?.length}
                    <div class="lsp-install">
                      <Button
                        size="sm"
                        variant="primary"
                        disabled={bennuLspStore.installing !== null}
                        loading={bennuLspStore.installing === srv.id}
                        onclick={() => void installServer(srv.id)}
                      >
                        {#snippet iconStart()}<Download size={13} />{/snippet}
                        {bennuLspStore.installing === srv.id ? 'Installing…' : 'Install'}
                      </Button>
                      <code class="lsp-install-cmd" use:tooltip={srv.install.join(' ')}>
                        {srv.install.join(' ')}
                      </code>
                      <CopyButton value={srv.install.join(' ')} title="Copy the install command" />
                    </div>
                  {/if}
                {/if}
                <label class="lsp-override">
                  <span>Executable path</span>
                  <Input
                    value={serverPaths[srv.id] ?? ''}
                    placeholder="leave empty to search PATH and the usual install locations"
                    onchange={(v) => void commitServerPath(srv.id, v)}
                  />
                </label>
              </div>
            {/each}
          {/if}
        </div>

        <div class="card">
          <div class="card-section-title"><Braces size={12} /> Adding a language</div>
          <p class="lsp-note">
            A language the catalogue does not cover is added in
            <code>bennu/config.toml</code> under <code>[[lsp.servers]]</code> — the same fields the
            built-in entries carry, so it gets the same features with no code change:
          </p>
          <pre class="lsp-sample">{`[[lsp.servers]]
id = "zls"
name = "Zig"
language = "zig"
command = "zls"
extensions = ["zig", "zon"]
root_markers = ["build.zig"]
initialization_options = ""`}</pre>
          <p class="lsp-note">
            <strong>root_markers</strong> is the gate: without one of those files above the file
            being edited there is no workspace to open, so nothing starts. An entry whose
            <code>id</code> matches a built-in replaces it.
          </p>
        </div>

      {:else if active === 'style'}
        <div class="section-header">
          <h2>Java Style</h2>
          <p>How the <strong>Generate</strong> flow formats the code it inserts.</p>
        </div>
        <div class="card">
          <div class="card-section-title"><Wand2 size={12} /> Declarations</div>
          <FormRow label="Final generated params and locals" description="Declare generated constructor, setter and with-method parameters as final.">
            <Toggle checked={s.finalParams} onchange={(v) => s.setFinalParams(v)} ariaLabel="Final generated params and locals" />
          </FormRow>
          <FormRow label="Use Lombok val for locals" description={hasLombok ? 'Prefer Lombok’s val for generated local variables.' : 'Requires Lombok on the classpath — not detected in this project.'}>
            <Toggle checked={s.useLombokVal} disabled={!hasLombok}
                    onchange={(v) => s.setUseLombokVal(v)} ariaLabel="Use Lombok val for locals" />
          </FormRow>
          <FormRow label="Switch with return" description="Prefer arrow-style switch expressions that return a value when generating switches.">
            <Toggle checked={s.switchWithReturn} onchange={(v) => s.setSwitchWithReturn(v)} ariaLabel="Switch with return" />
          </FormRow>
        </div>
        <div class="card">
          <div class="card-section-title"><Wand2 size={12} /> Spacing</div>
          <FormRow label="Spaces inside braces" description="Add a space just inside the braces on single-line generated bodies.">
            <Toggle checked={s.spaceInBraces} onchange={(v) => s.setSpaceInBraces(v)} ariaLabel="Spaces inside braces" />
          </FormRow>
          <FormRow label="Blank line between members" description="Separate each generated method with a blank line.">
            <Toggle checked={s.blankLineBetweenMembers} onchange={(v) => s.setBlankLineBetweenMembers(v)} ariaLabel="Blank line between members" />
          </FormRow>
        </div>
        <div class="card">
          <div class="card-section-title"><Wand2 size={12} /> Formatter</div>
          <p class="bs-hint">
            What <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> does to a <code>.java</code> file.
            Indentation comes from Editor → Indentation, so the formatter and the editor never
            disagree. A language with a language server is formatted by it instead, reading the
            project's own <code>rustfmt.toml</code> or <code>.prettierrc</code> — nothing here
            applies to those.
          </p>
          <FormRow label="Blank lines between members" description="The most consecutive blank lines the formatter keeps. 0 removes them all.">
            <NumberStepper value={s.javaBlankLines} min={0} max={5} narrow
                           onchange={(v) => s.setJavaBlankLines(v)} ariaLabel="Blank lines between members" />
          </FormRow>
          <FormRow label="Indent case bodies" description="Indent the statements under a case label one level in from it — the Sun/Oracle convention, and IntelliJ's.">
            <Toggle checked={s.javaIndentCaseBody} onchange={(v) => s.setJavaIndentCaseBody(v)} ariaLabel="Indent case bodies" />
          </FormRow>
        </div>
        <div class="card">
          <div class="card-section-title"><Wand2 size={12} /> Preview</div>
          <pre class="bs-snippet" aria-label="Java style preview">{styleSnippet}</pre>
        </div>

      {:else if active === 'java'}
        <div class="section-header">
          <h2>Java</h2>
          <p>How Java sources are decoded and indexed.</p>
        </div>
        <div class="card">
          <div class="card-section-title"><Braces size={12} /> Sources</div>
          <FormRow label="Default source encoding" description="Fallback when the pom doesn't declare project.build.sourceEncoding.">
            <Select value={s.defaultEncoding} options={encodingOptions}
                    onchange={(v) => s.setDefaultEncoding(v as SourceEncoding)} />
          </FormRow>
          <FormRow label="Rebuild index on open" description="Re-scan symbols each time a project opens (slower open, fresher completion).">
            <Toggle checked={s.rebuildIndexOnOpen} onchange={(v) => s.setRebuildIndexOnOpen(v)} ariaLabel="Rebuild index on open" />
          </FormRow>
          <FormRow label="Validate project on open" description="After indexing, validate the whole project in the background so the first ‘Validate (no compile)’ is instant. Uses a little CPU on open.">
            <Toggle checked={validateOnOpen} onchange={(v) => commitValidateOnOpen(v)} ariaLabel="Validate project on open" />
          </FormRow>
          <FormRow label="Validation CPU threads" description="Max worker threads the whole-project validation may use. 0 = auto (leaves about half the cores free for the UI). Set 1 for single-threaded so a big project can’t peg every core and freeze the editor.">
            <Input value={String(validationThreads)} placeholder="0"
                   onchange={(v) => commitValidationThreads(v)} ariaLabel="Validation CPU threads" />
          </FormRow>
          <FormRow label="Indexing CPU threads" description="Max worker threads the index build, the find-usages reference walk and the encoding scan may use. 1 = serial, the default — indexing is a background job and one that makes the machine unusable has not earned its speed. Raise it when indexing feels slow and there are cores to spare; 0 = auto.">
            <Input value={String(indexThreads)} placeholder="1"
                   onchange={(v) => commitIndexThreads(v)} ariaLabel="Indexing CPU threads" />
          </FormRow>
          <FormRow label="Excluded directories" description="Comma-separated folder names skipped by the indexer.">
            <Input value={s.excludedDirs} placeholder="target, .git"
                   onchange={(v) => s.setExcludedDirs(v)} ariaLabel="Excluded directories" />
          </FormRow>
        </div>
        <div class="card">
          <div class="card-section-title"><Package size={12} /> Navigation</div>
          <FormRow
            label="Search the dependencies too"
            description="Open Go to (Ctrl+N / Ctrl+Shift+N) and Find in project on Project & dependencies rather than on Project alone, so a framework annotation, a struts-default.xml or a schema is found without asking for it. Either way the Source picker still decides the search in front of you. The jars are searched as you type rather than listed, so they cost nothing until used; the first search after opening a project spends a moment reading them."
          >
            <Toggle checked={s.searchDependencies} onchange={(v) => void s.setSearchDependencies(v)}
                    ariaLabel="Search the dependencies too" />
          </FormRow>
        </div>
        {#if s.excludedDirList.length}
          <div class="card">
            <div class="card-section-title"><Braces size={12} /> Parsed exclusions ({s.excludedDirList.length})</div>
            <div class="bs-chips">
              {#each s.excludedDirList as d (d)}
                <Badge variant="tone" tone="neutral" label={d} />
              {/each}
            </div>
          </div>
        {/if}

      {:else if !project}
        <EmptyState message="Open a project to see its resolved settings." />
      {:else if active === 'debugger'}
        <div class="section-header">
          <h2>Debugger</h2>
          <p>What a step walks through, and what it walks past.</p>
        </div>
        <div class="card">
          <div class="card-section-title"><Bug size={12} /> Step-through packages</div>
          <p class="bs-hint">
            Classes a <strong>step into</strong> passes straight through instead of stopping in.
            Without them, stepping into <code>service.place(order)</code> walks the proxy, then
            <code>ReflectiveMethodInvocation.proceed</code>, then every interceptor in the chain —
            a dozen stops in code you have no source for. Remove <code>org.springframework.*</code>
            to be able to step into Spring itself.
            A <code>*</code> is allowed at <strong>one end only</strong>.
          </p>
          {#if usingDefaults}
            <p class="bs-none">These are the defaults. Adding or removing one makes the list yours.</p>
          {/if}
          <div class="bs-paths">
            {#each stepExcludes as p (p)}
              <div class="bs-path">
                <span class="bs-path-txt bs-mono" use:tooltip={p}>{p}</span>
                <button class="bs-path-del" type="button" onclick={() => void removeExclude(p)} aria-label="Remove pattern"><Trash2 size={13} /></button>
              </div>
            {/each}
          </div>
          <div class="bs-exclude-add">
            <Input
              bind:value={excludeDraft}
              placeholder="com.acme.generated.*"
              size="sm"
              error={draftValid ? null : 'A * is allowed at one end only'}
              onkeydown={(e: KeyboardEvent) => { if (e.key === 'Enter') void addExclude(); }}
            />
            <Button variant="ghost" size="sm" disabled={!excludeDraft.trim() || !draftValid} onclick={() => void addExclude()}>
              {#snippet iconStart()}<Plus size={13} />{/snippet}
              Add
            </Button>
            <Button variant="ghost" size="sm" disabled={usingDefaults} onclick={() => void resetExcludes()}>
              Reset
            </Button>
          </div>
          {#if !draftValid}
            <p class="bs-invalid">A pattern may carry a <code>*</code> at one end only — the VM refuses anything else, and one bad entry stops stepping working at all.</p>
          {/if}
        </div>
      {:else if active === 'jdk'}
        <div class="section-header">
          <h2>JDK</h2>
          <p>The JDK Bennu resolves the standard library against for completion and navigation.</p>
        </div>
        <div class="card">
          <div class="card-section-title"><Coffee size={12} /> Resolved JDK</div>
          {#if jdk}
            <div class="bs-kv"><span class="bs-k">Project targets</span><span class="bs-v">Java {jdk.version} <span class="bs-muted">({jdk.source})</span></span></div>
          {:else}
            <p class="bs-none">No language level inferred from the pom — defaulting to Java 8.</p>
          {/if}
          {#if jdkReport}
            {#if !jdkReport.any_installed}
              <div class="bs-warn bs-warn-error"><TriangleAlert size={13} /> No JDK found — completion and navigation can’t resolve the standard library. Add a JDK directory below.</div>
            {:else if !jdkReport.exact}
              <div class="bs-warn"><TriangleAlert size={13} /> No JDK for the exact level installed — using Java {jdkReport.resolved_major} as a fallback.</div>
              {#if jdkReport.resolved_home}<div class="bs-kv"><span class="bs-k">Using</span><span class="bs-v"><code>{jdkReport.resolved_home}</code></span></div>{/if}
            {:else if jdkReport.resolved_home}
              <div class="bs-kv"><span class="bs-k">Using</span><span class="bs-v"><code>{jdkReport.resolved_home}</code></span></div>
            {/if}
          {/if}
        </div>
        <div class="card">
          <div class="card-section-title"><FolderOpen size={12} /> Search paths</div>
          <p class="bs-hint">Extra JDK install directories, searched before <code>JAVA_HOME</code> and the standard install roots — for a JDK installed somewhere non-standard. On macOS either the <code>.jdk</code> bundle or the <code>Contents/Home</code> inside it works.</p>
          {#if jdkPaths.length}
            <div class="bs-paths">
              {#each jdkPaths as p (p)}
                <div class="bs-path">
                  <span class="bs-path-txt" use:tooltip={p}>{p}</span>
                  <button class="bs-path-del" type="button" onclick={() => removeJdkPath(p)} aria-label="Remove JDK path"><Trash2 size={13} /></button>
                </div>
              {/each}
            </div>
          {:else}
            <p class="bs-none">No extra paths — only JAVA_HOME and the standard roots are searched.</p>
          {/if}
          <div class="bs-path-add">
            <Button variant="ghost" size="sm" onclick={() => (jdkPickerOpen = true)}>
              {#snippet iconStart()}<Plus size={13} />{/snippet}
              Add JDK directory…
            </Button>
          </div>
        </div>
      {:else if active === 'capabilities'}
        <div class="section-header">
          <h2>Capabilities</h2>
          <p>The domain frameworks detected in this project, with the evidence that activated each.</p>
        </div>
        <div class="card">
          <div class="card-section-title"><Boxes size={12} /> Detected ({enabledCaps.length})</div>
          {#if enabledCaps.length === 0}
            <p class="bs-none">No domain capabilities detected.</p>
          {:else}
            <div class="bs-chips">
              {#each enabledCaps as c (c)}
                <Badge variant="tone" tone="accent" label={capLabel(c)} />
              {/each}
            </div>
          {/if}
        </div>
        {#if hits.length}
          <div class="card">
            <div class="card-section-title"><Boxes size={12} /> Evidence</div>
            {#each hits as h, i (i)}
              <div class="bs-hit">
                <span class="bs-tier bs-tier-{h.tier.toLowerCase()}">{h.tier}</span>
                <span class="bs-hit-body">
                  <span class="bs-hit-cap">{capLabel(h.capability)}</span>
                  <span class="bs-hit-detail">{h.detail}</span>
                </span>
              </div>
            {/each}
          </div>
        {/if}
      {:else if active === 'encoding'}
        <div class="section-header">
          <h2>Encoding</h2>
          <p>How file source is decoded. Legacy projects often declare <code>Cp1252</code> in the pom.</p>
        </div>
        <div class="card">
          <div class="card-section-title"><FileType size={12} /> Active file</div>
          {#if projectStore.activeFilePath}
            <div class="bs-kv"><span class="bs-k">File</span><span class="bs-v">{projectStore.activeFilePath.split(/[\\/]/).pop()}</span></div>
            <div class="bs-kv"><span class="bs-k">Decoded as</span><span class="bs-v">{projectStore.activeEncoding}</span></div>
          {:else}
            <p class="bs-none">Open a file to see the encoding it was decoded from.</p>
          {/if}
        </div>
      {:else if active === 'beans'}
        <div class="section-header">
          <h2>Beans</h2>
          <p>
            Which dependencies contribute their Spring beans to the <strong>Library beans</strong>
            view. Nothing is read until you name something here.
          </p>
        </div>
        <div class="card">
          <div class="card-section-title"><Boxes size={12} /> Read beans from these dependencies</div>
          <p class="bs-none">
            Any match admits an artifact. The intended entries are your <strong>own</strong> shared
            modules and starters — their beans are plain <code>@Service</code> /
            <code>@Configuration</code> and simply true. Spring Boot's own starters can be added,
            but their beans are conditional and are shown as such.
          </p>
          {#each beanAxes as axis (axis.key)}
            <div class="bs-field">
              <label class="bs-k" for="beans-{axis.key}">{axis.label}</label>
              <Input
                id="beans-{axis.key}"
                value={(libraryBeans[axis.key] ?? []).join(', ')}
                placeholder={axis.hint}
                onchange={(v: string) => void commitBeanAxis(axis.key, v)}
              />
            </div>
          {/each}
          {#if beansAllowlistEmpty}
            <p class="bs-none">
              Empty — no dependency jar is opened, and the Library beans view stays empty.
            </p>
          {/if}
        </div>
        <div class="card">
          <div class="card-section-title"><Boxes size={12} /> What this view is not</div>
          <p class="bs-none">
            A bean declared inside a jar is what Spring <em>may</em> register:
            <code>@ConditionalOnMissingBean</code> and its family decide the rest, and deciding them
            faithfully means running Spring's own evaluator. So these beans are listed and navigable,
            each labelled with the conditions gating it — and they take no part in autowiring
            candidates, completion, or any diagnostic. Your project's own beans remain the only
            answer to “what does this application have”.
          </p>
        </div>
      {/if}
    {/snippet}
  </SettingsShell>

  {#snippet footer()}
    <ModalFooter align="between">
      <Button variant="ghost" size="sm" onclick={() => bennuSettingsStore.resetToDefaults()}>
        {#snippet iconStart()}<RotateCcw size={13} />{/snippet}
        Reset to defaults
      </Button>
      <Button variant="primary" size="sm" onclick={onClose}>Done</Button>
    </ModalFooter>
  {/snippet}
</Modal>

{#if jdkPickerOpen}
  <FileExplorerModal
    mode="folder"
    title="Select a JDK install directory"
    onConfirm={onPickJdk}
    onCancel={() => (jdkPickerOpen = false)}
    onClose={() => (jdkPickerOpen = false)}
  />
{/if}

{#if themeEditorOpen}
  <ThemeEditorModal onClose={() => (themeEditorOpen = false)} />
{/if}

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }

  /* ── Language servers ── */
  .lsp-run, .lsp-srv {
    display: flex; flex-direction: column; gap: 6px;
    padding: 9px 10px; background: var(--bg-base);
    border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
  }
  .lsp-run { flex-direction: row; align-items: flex-start; gap: 10px; }
  .lsp-run + .lsp-run, .lsp-srv + .lsp-srv { margin-top: 6px; }
  .lsp-run-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .lsp-run-head, .lsp-srv-head { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .lsp-run-actions { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }
  .lsp-name { font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary); }
  .lsp-exts { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); }
  .lsp-spacer { flex: 1; }
  .lsp-progress { font-size: var(--font-size-xs); color: var(--accent); }
  .lsp-run-sub, .lsp-srv-path {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-disabled);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .lsp-run-msg { font-size: var(--font-size-xs); color: var(--warning); line-height: 1.4; }
  .lsp-install { display: flex; align-items: center; gap: 10px; margin-top: 6px; }
  .lsp-install-cmd {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-faint);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }

  .lsp-srv-hint { font-size: var(--font-size-xs); color: var(--text-muted); line-height: 1.45; }
  .lsp-srv-hint code, .lsp-note code { font-family: var(--font-code); color: var(--text-primary); }
  .lsp-log, .lsp-sample {
    margin: 0; padding: 7px 9px; max-height: 140px; overflow: auto;
    font-family: var(--font-code); font-size: var(--font-size-2xs); line-height: 1.5;
    color: var(--text-muted); background: var(--bg-elevated);
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
    white-space: pre;
  }
  .lsp-override { display: flex; align-items: center; gap: 8px; }
  .lsp-override > span { font-size: var(--font-size-xs); color: var(--text-muted); flex-shrink: 0; }
  .lsp-override :global(.input-wrap) { flex: 1; }
  .lsp-note {
    margin: 0; padding: 2px 2px 6px;
    font-size: var(--font-size-xs); color: var(--text-muted); line-height: 1.5;
  }
  :global(.lsp-ok) { color: var(--success); flex-shrink: 0; }
  :global(.lsp-warn) { color: var(--warning); flex-shrink: 0; }
  .bs-muted { color: var(--text-muted); }
  .bs-hint { font-size: var(--font-size-xs); color: var(--text-muted); line-height: 1.45; padding: 4px 2px 8px; }
  .bs-warn {
    display: flex; align-items: center; gap: 7px; margin: 8px 2px 2px;
    padding: 7px 10px; font-size: var(--font-size-sm); line-height: 1.4;
    color: var(--warning); background: color-mix(in srgb, var(--warning) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--warning) 30%, transparent); border-radius: var(--radius-md);
  }
  .bs-warn :global(svg) { flex-shrink: 0; }
  .bs-warn-error {
    color: var(--error); background: color-mix(in srgb, var(--error) 12%, transparent);
    border-color: color-mix(in srgb, var(--error) 30%, transparent);
  }
  .bs-paths { display: flex; flex-direction: column; gap: 4px; padding: 4px 2px; }
  .bs-path {
    display: flex; align-items: center; gap: 8px;
    padding: 5px 8px; background: var(--bg-base);
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
  }
  .bs-path-txt {
    flex: 1; min-width: 0; font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left;
  }
  .bs-path-del {
    display: flex; flex-shrink: 0; padding: 3px; background: transparent; border: none;
    color: var(--text-muted); cursor: pointer; border-radius: var(--radius-sm);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .bs-path-del:hover { color: var(--error); background: var(--bg-hover); }
  .bs-path-add { padding: 6px 2px 2px; }
  /* The pattern editor: a field that takes the width, then Add and Reset. */
  .bs-exclude-add { display: flex; align-items: center; gap: 6px; padding: 6px 2px 2px; }
  .bs-exclude-add :global(.input-wrap) { flex: 1; min-width: 0; }
  .bs-mono { font-family: var(--font-code); }
  .bs-invalid { font-size: var(--font-size-xs); color: var(--error); padding: 2px 2px 4px; }
  .bs-kv { display: flex; align-items: center; gap: 10px; padding: 6px 2px; font-size: var(--font-size-sm); }
  /* Same row shape as `.bs-kv`, but the value is an editable field that has to take the
     remaining width — a coordinate list is long and reading it half-clipped is useless. */
  .bs-field { display: flex; align-items: center; gap: 10px; padding: 6px 2px; font-size: var(--font-size-sm); }
  .bs-field :global(.input-wrap) { flex: 1; min-width: 0; }
  .bs-k { width: 110px; flex-shrink: 0; color: var(--text-muted); }
  .bs-v { color: var(--text-primary); }
  .bs-none { font-size: var(--font-size-sm); color: var(--text-muted); font-style: italic; padding: 4px 2px; }
  .bs-chips { display: flex; flex-wrap: wrap; gap: 6px; padding: 10px 14px; }
  .bs-hit { display: flex; align-items: flex-start; gap: 10px; padding: 7px 2px; border-top: 1px solid var(--border-subtle); }
  .bs-hit:first-of-type { border-top: none; }
  .bs-tier {
    flex-shrink: 0; width: 18px; height: 18px; border-radius: var(--radius-sm);
    display: flex; align-items: center; justify-content: center;
    font-size: var(--font-size-2xs); font-weight: 700;
  }
  .bs-tier-a { color: var(--success); background: color-mix(in srgb, var(--success) 18%, transparent); }
  .bs-tier-b { color: var(--info);    background: color-mix(in srgb, var(--info) 18%, transparent); }
  .bs-tier-c { color: var(--warning); background: color-mix(in srgb, var(--warning) 18%, transparent); }
  .bs-hit-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .bs-hit-cap { font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary); }
  .bs-hit-detail { font-size: var(--font-size-xs); color: var(--text-muted); }

  /* Read-only live-preview snippets (Editor / Java Style cards). */
  .bs-snippet {
    margin: 10px 14px 12px;
    padding: 10px 12px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    font-family: var(--font-code);
    font-size: var(--font-size-sm);
    line-height: 1.55;
    color: var(--text-primary);
    white-space: pre;
    overflow-x: auto;
    user-select: text;
  }
  /* Positioning context for the Editor preview's margin guide. */
  .bs-snippet-wrap { position: relative; }
  .bs-snippet-ruler {
    position: absolute;
    top: 10px;
    bottom: 12px;
    width: 1px;
    background: var(--border-focus);
    opacity: 0.4;
    pointer-events: none;
  }
</style>
