<script lang="ts">
  /**
   * Settings › Editor — how source is drawn, how Tab indents, and what `.sql` is coloured as.
   *
   * Everything here is the editing surface itself, which is why it is one page on every project
   * kind: a Cargo project draws its buffers with the same rules a Maven one does.
   */
  import { Database, TextCursorInput } from 'lucide-svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import {
    bennuSettingsStore, SQL_DIALECTS,
    type IndentStyle, type SqlDialectSetting,
  } from '$lib/stores/bennu/settings.svelte';

  const s = bennuSettingsStore;

  const indentOptions = [
    { value: 'spaces', label: 'Spaces' },
    { value: 'tabs',   label: 'Tabs' },
  ];

  /** Engine labels, not the raw setting values — "postgres" in a dropdown reads like a hostname.
   *  `portable` leads: it is the default and the safe answer. */
  const SQL_DIALECT_LABELS: Record<SqlDialectSetting, string> = {
    portable: 'Portable (both engines)',
    oracle: 'Oracle / PL-SQL',
    postgres: 'PostgreSQL',
  };
  const sqlDialectOptions = SQL_DIALECTS.map((d) => ({ value: d, label: SQL_DIALECT_LABELS[d] }));

  /** One indent unit: real tabs when the indent style is tabs, else N spaces — so the preview's
   *  indentation tracks the settings above it faithfully. */
  const indentUnit = $derived(s.indentStyle === 'tabs' ? '\t' : ' '.repeat(s.tabSize));

  const editorSnippet = $derived.by(() => {
    const i = indentUnit;
    const raw = [
      'void demo() {',
      `${i}int total = compute();`,
      `${i}${i}// nested`,
      '}',
    ].join('\n');
    // showWhitespace → the same dot/arrow glyphs the editor would draw. Preview-only, so the real
    // source text is never mutated.
    if (!s.showWhitespace) return raw;
    return raw.replace(/\t/g, '→').replace(/ /g, '·');
  });

  const editorRuler = $derived(s.rightMargin > 0);
</script>

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
  <FormRow label="Usage counts" description="Draw how many places use each class, method and field above it, and fade the name of one nothing reaches. Java only — it is the whole-project reference index answering, so it says nothing until the index is built. A member carrying an annotation, one that overrides something, and main are never faded: a framework can reach any of them by name, and an index cannot see that.">
    <Toggle checked={s.usageCounts} onchange={(v) => s.setUsageCounts(v)} ariaLabel="Usage counts" />
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
  <div class="set-snippet-wrap">
    {#if editorRuler}
      <!-- Margin guide: a thin rule at the configured column, tabSize-relative -->
      <span class="set-snippet-ruler" style="left: calc({s.rightMargin}ch + 12px);" aria-hidden="true"></span>
    {/if}
    <pre class="set-snippet" style="tab-size: {s.tabSize};" aria-label="Editor preview">{editorSnippet}</pre>
  </div>
</div>
