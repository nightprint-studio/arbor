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
   * active file's encoding. No theme section here — theme lives in the titlebar
   * gear submenu.
   */
  import {
    Settings, Coffee, Boxes, FileType, TextCursorInput, ListTree,
    FoldVertical, Braces, RotateCcw, Wand2, Plus, Trash2, TriangleAlert, FolderOpen,
    Database,
  } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import SettingsShell, { type SettingsNavGroup } from '$lib/components/shared/ui/SettingsShell.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
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
  import { getBennuConfig, setBennuConfig, type BennuConfig } from '$lib/ipc/bennu/config';

  let { onClose }: { onClose: () => void } = $props();

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

  /** The Java-only sections drop out on a Cargo project: JDK, Capabilities and the
   *  Java/Java-Style pages are each a statement about a Java stack, and the encoding page
   *  has nothing to resolve (Rust is UTF-8 by definition). Editor / Completion / Folding
   *  apply to every buffer and stay. */
  const groups = $derived<SettingsNavGroup[]>([
    { label: 'Editor', items: [
      { id: 'editor',     label: 'Editor',     icon: TextCursorInput },
      { id: 'completion', label: 'Completion', icon: ListTree },
      { id: 'folding',    label: 'Folding',    icon: FoldVertical },
      ...(projectStore.isCargo ? [] : [
        { id: 'style',    label: 'Java Style', icon: Wand2 },
        { id: 'java',     label: 'Java',       icon: Braces },
      ]),
    ] },
    ...(projectStore.isCargo ? [] : [{
      label: 'Project', items: [
        { id: 'jdk',          label: 'JDK',          icon: Coffee },
        { id: 'capabilities', label: 'Capabilities', icon: Boxes },
        { id: 'encoding',     label: 'Encoding',     icon: FileType },
      ],
    }]),
  ]);
  let active = $state('editor');

  // A section that just disappeared (project switched to Cargo while it was open) would
  // leave the shell on a page with no nav entry. Fall back to Editor.
  $effect(() => {
    const ids = groups.flatMap((g) => g.items.map((i) => i.id));
    if (!ids.includes(active)) active = 'editor';
  });

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
      {#if active === 'editor'}
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
          <FormRow label="Excluded directories" description="Comma-separated folder names skipped by the indexer.">
            <Input value={s.excludedDirs} placeholder="target, .git"
                   onchange={(v) => s.setExcludedDirs(v)} ariaLabel="Excluded directories" />
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
          <p class="bs-hint">Extra JDK install directories, searched on top of <code>JAVA_HOME</code> and the standard install roots — for a JDK installed somewhere non-standard.</p>
          {#if jdkPaths.length}
            <div class="bs-paths">
              {#each jdkPaths as p (p)}
                <div class="bs-path">
                  <span class="bs-path-txt" title={p}>{p}</span>
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

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
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
  .bs-kv { display: flex; align-items: center; gap: 10px; padding: 6px 2px; font-size: var(--font-size-sm); }
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
