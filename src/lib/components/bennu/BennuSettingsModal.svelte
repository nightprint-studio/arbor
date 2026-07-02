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
    FoldVertical, Braces, RotateCcw, Wand2,
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
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuSettingsStore, SOURCE_ENCODINGS, type IndentStyle, type SourceEncoding } from '$lib/stores/bennu/settings.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const groups: SettingsNavGroup[] = [
    { label: 'Editor', items: [
      { id: 'editor',     label: 'Editor',     icon: TextCursorInput },
      { id: 'completion', label: 'Completion', icon: ListTree },
      { id: 'folding',    label: 'Folding',    icon: FoldVertical },
      { id: 'style',      label: 'Java Style', icon: Wand2 },
      { id: 'java',       label: 'Java',       icon: Braces },
    ] },
    { label: 'Project', items: [
      { id: 'jdk',          label: 'JDK',          icon: Coffee },
      { id: 'capabilities', label: 'Capabilities', icon: Boxes },
      { id: 'encoding',     label: 'Encoding',     icon: FileType },
    ] },
  ];
  let active = $state('editor');

  const s = bennuSettingsStore;

  const indentOptions = [
    { value: 'spaces', label: 'Spaces' },
    { value: 'tabs',   label: 'Tabs' },
  ];
  const encodingOptions = SOURCE_ENCODINGS.map((e) => ({ value: e, label: e }));

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
          <FormRow label="Show line numbers" description="Gutter line numbers on the left margin.">
            <Toggle checked={s.showLineNumbers} onchange={(v) => s.setShowLineNumbers(v)} ariaLabel="Show line numbers" />
          </FormRow>
          <FormRow label="Highlight current line" description="Tint the line the caret sits on.">
            <Toggle checked={s.highlightCurrentLine} onchange={(v) => s.setHighlightCurrentLine(v)} ariaLabel="Highlight current line" />
          </FormRow>
          <FormRow label="Word wrap" description="Wrap long lines to the viewport instead of scrolling horizontally.">
            <Toggle checked={s.wordWrap} onchange={(v) => s.setWordWrap(v)} ariaLabel="Word wrap" />
          </FormRow>
          <FormRow label="Show whitespace" description="Render dots and arrows for spaces and tabs.">
            <Toggle checked={s.showWhitespace} onchange={(v) => s.setShowWhitespace(v)} ariaLabel="Show whitespace" />
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
          <p>The Java language level Bennu resolves the classpath against.</p>
        </div>
        <div class="card">
          <div class="card-section-title"><Coffee size={12} /> Resolved JDK</div>
          {#if jdk}
            <div class="bs-kv"><span class="bs-k">Version</span><span class="bs-v">{jdk.version}</span></div>
            <div class="bs-kv"><span class="bs-k">Source</span><span class="bs-v"><code>{jdk.source}</code></span></div>
          {:else}
            <p class="bs-none">Not inferred — set a compiler source/target in the pom, or an override.</p>
          {/if}
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

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .bs-kv { display: flex; align-items: center; gap: 10px; padding: 6px 2px; font-size: 12.5px; }
  .bs-k { width: 110px; flex-shrink: 0; color: var(--text-muted); }
  .bs-v { color: var(--text-primary); }
  .bs-none { font-size: 12px; color: var(--text-muted); font-style: italic; padding: 4px 2px; }
  .bs-chips { display: flex; flex-wrap: wrap; gap: 6px; padding: 10px 14px; }
  .bs-hit { display: flex; align-items: flex-start; gap: 10px; padding: 7px 2px; border-top: 1px solid var(--border-subtle); }
  .bs-hit:first-of-type { border-top: none; }
  .bs-tier {
    flex-shrink: 0; width: 18px; height: 18px; border-radius: var(--radius-sm);
    display: flex; align-items: center; justify-content: center;
    font-size: 10px; font-weight: 700;
  }
  .bs-tier-a { color: var(--success); background: color-mix(in srgb, var(--success) 18%, transparent); }
  .bs-tier-b { color: var(--info);    background: color-mix(in srgb, var(--info) 18%, transparent); }
  .bs-tier-c { color: var(--warning); background: color-mix(in srgb, var(--warning) 18%, transparent); }
  .bs-hit-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .bs-hit-cap { font-size: 12.5px; font-weight: 600; color: var(--text-primary); }
  .bs-hit-detail { font-size: 11.5px; color: var(--text-muted); }
</style>
