<script lang="ts">
  /**
   * BennuRegexModal — the Regex Tester.
   *
   * Opens empty (palette) or off a string literal in the editor (context menu / shortcut), in which
   * case the pattern is the literal's *value* — escapes undone — and "Apply to source" writes the
   * edited pattern back with the literal's own escaping.
   *
   * The pattern field paints its own syntax and underlines what the parser rejects; the test text paints
   * every match with a tint per capture group; the side panel lists the matches as a table, explains the
   * pattern token by token (hover a row and the token lights up in the field, move the caret and the row
   * follows) and keeps saved patterns. Syntax reference is a quick-insert menu, not a pane.
   *
   * Every run goes through a worker with a deadline (`regex/run.ts`), so a pattern that backtracks
   * forever shows a "timed out" notice and a working UI instead of a frozen window. The flavour decides
   * both the syntax the pattern is read under and the engine: Java and Rust/PCRE run on Arbor's own
   * matcher (Java semantics for Java), JavaScript on the host's `RegExp`.
   *
   * Keyboard-first: Ctrl/Cmd+Enter applies to the source (or copies the first snippet when opened
   * empty), Alt+1…3 switch the side panel, F (outside a field) focuses the pattern, Esc closes.
   */
  import { onDestroy, tick, untrack } from 'svelte';
  import { Regex, Copy, Check, History } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import ToggleButton from '$lib/components/shared/ui/ToggleButton.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { bennuRegexStore, type KeptPattern } from '$lib/stores/bennu/regex.svelte';
  import { parseRegex } from './regex/parse';
  import { explainParsed } from './regex/explain';
  import { exportsFor } from './regex/export-code';
  import { createRegexRunner, type RunOutcome } from './regex/run';
  import { insertion } from './regex/reference';
  import { segmentsOf } from './regex/segments';
  import { regexSpans } from './regex/tokens';
  import { FLAVOUR_LABELS, type Flavour, type MatchMode, type RunRequest } from './regex/types';
  import RegexPatternField from './regex/RegexPatternField.svelte';
  import RegexTestText from './regex/RegexTestText.svelte';
  import RegexMatchList from './regex/RegexMatchList.svelte';
  import RegexExplain from './regex/RegexExplain.svelte';
  import RegexInsertMenu from './regex/RegexInsertMenu.svelte';
  import RegexLibrary from './regex/RegexLibrary.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const initial = untrack(() => bennuRegexStore.request);
  const target = initial?.target ?? null;

  // The flags each flavour understands, with what each one does (for the key's tooltip).
  const FLAGS: Record<Flavour, { key: string; title: string }[]> = {
    java: [
      { key: 'i', title: 'Case-insensitive' }, { key: 'm', title: 'Multi-line: ^ and $ at every line' },
      { key: 's', title: 'Dot matches line breaks' }, { key: 'x', title: 'Free-spacing: ignore whitespace and # comments' },
      { key: 'u', title: 'Unicode case folding' }, { key: 'U', title: 'Unicode character classes' },
      { key: 'd', title: 'Unix lines: only \\n ends a line' },
    ],
    js: [
      { key: 'g', title: 'Global: every match' }, { key: 'i', title: 'Case-insensitive' },
      { key: 'm', title: 'Multi-line: ^ and $ at every line' }, { key: 's', title: 'Dot matches line breaks' },
      { key: 'u', title: 'Unicode mode' },
    ],
    rust: [
      { key: 'i', title: 'Case-insensitive' }, { key: 'm', title: 'Multi-line: ^ and $ at every line' },
      { key: 's', title: 'Dot matches line breaks' }, { key: 'x', title: 'Free-spacing: ignore whitespace and # comments' },
      { key: 'u', title: 'Unicode mode' }, { key: 'U', title: 'Swap greedy and lazy' },
    ],
  };

  const flavourOptions = (Object.keys(FLAVOUR_LABELS) as Flavour[]).map((f) => ({ value: f, label: FLAVOUR_LABELS[f] }));
  const modeOptions = [
    { value: 'find', label: 'Find all' },
    { value: 'matches', label: 'Whole text' },
    { value: 'lookingAt', label: 'At start' },
  ];

  let flavour = $state<Flavour>(initial?.flavour ?? 'java');
  let pattern = $state(initial?.pattern ?? '');
  let flags = $state(initial?.flags ?? (initial?.flavour === 'js' ? 'g' : ''));
  let text = $state('');
  let replacement = $state('');
  let useReplacement = $state(false);
  let mode = $state<MatchMode>('find');
  let sideTab = $state('matches');
  let patternEl = $state<HTMLInputElement>();
  let copied = $state<string | null>(null);
  /** Ranges emphasised from the other pane: a token hovered in the explanation, a match hovered in the table. */
  let patternHover = $state<{ from: number; to: number } | null>(null);
  let textHover = $state<{ from: number; to: number } | null>(null);
  let caret = $state<number | null>(null);

  // Reopening a pattern tried before brings its text and replacement back with it.
  untrack(() => {
    const seen = [...bennuRegexStore.recent, ...bennuRegexStore.saved]
      .find((k) => k.pattern === pattern && k.flavour === flavour);
    if (seen) { text = seen.text; replacement = seen.replacement; useReplacement = seen.replacement !== ''; }
  });

  $effect(() => { patternEl?.focus(); });

  // ── Syntax (instant, on this thread: parsing is linear) ──────────────────────────
  const parsed = $derived(parseRegex(pattern, flavour, flags));
  const spans = $derived(regexSpans(parsed));
  const explanation = $derived(explainParsed(pattern, parsed));
  const syntaxError = $derived(parsed.errors[0] ?? null);

  // ── Matching (worker, debounced) ─────────────────────────────────────────────────
  const runner = createRegexRunner();
  onDestroy(() => runner.dispose());
  let outcome = $state<RunOutcome | null>(null);
  let running = $state(false);
  let slow = $state(false);
  let runSeq = 0;

  $effect(() => {
    const req: RunRequest = {
      flavour, pattern, flags, text, mode,
      replacement: useReplacement ? replacement : null,
      maxMatches: 5000,
    };
    if (!pattern || syntaxError) { runner.cancel(); outcome = null; running = false; slow = false; return; }
    const slowTimer = setTimeout(() => { if (running) slow = true; }, 250);
    const timer = setTimeout(() => {
      const mine = ++runSeq;
      running = true;
      void runner.run(req, 2000).then((o) => {
        if (mine !== runSeq) return;
        running = false;
        slow = false;
        if (o.kind !== 'cancelled') outcome = o;
      });
    }, 120);
    return () => { clearTimeout(timer); clearTimeout(slowTimer); };
  });

  const result = $derived(outcome?.kind === 'done' ? outcome.result : null);
  const segments = $derived(segmentsOf(text, result?.matches ?? [], textHover));

  // ── Actions ──────────────────────────────────────────────────────────────────────
  const snapshot = (): KeptPattern => ({ name: '', pattern, flags, flavour, text, replacement: useReplacement ? replacement : '' });

  function remember() { if (pattern) bennuRegexStore.remember(snapshot()); }

  function load(p: KeptPattern) {
    flavour = (['java', 'js', 'rust'] as const).find((f) => f === p.flavour) ?? 'java';
    pattern = p.pattern; flags = p.flags; text = p.text;
    replacement = p.replacement; useReplacement = p.replacement !== '';
  }

  function toggleFlag(key: string) {
    flags = flags.includes(key) ? flags.replace(key, '') : flags + key;
  }

  function setFlavour(f: string) {
    const next = f as Flavour;
    const allowed = new Set(FLAGS[next].map((x) => x.key));
    flags = [...flags].filter((c) => allowed.has(c)).join('');
    if (next === 'js' && !flags.includes('g')) flags += 'g';
    flavour = next;
  }

  /** Write a quick-insert token at the caret (replacing the selection) and leave the caret inside its gap. */
  async function insertToken(token: string) {
    const el = patternEl;
    const { text: add, back } = insertion(token);
    const from = el?.selectionStart ?? pattern.length;
    const to = el?.selectionEnd ?? pattern.length;
    pattern = pattern.slice(0, from) + add + pattern.slice(to);
    await tick();
    el?.focus();
    const at = from + add.length - back;
    el?.setSelectionRange(at, at);
    caret = at;
  }

  async function copy(label: string, code: string) {
    if (await copyToClipboard(code)) {
      copied = label;
      remember();
      setTimeout(() => { if (copied === label) copied = null; }, 1500);
    }
  }

  const copyItems = $derived<DropdownItem[]>(
    exportsFor(pattern, flavour, flags).map((e) => ({
      kind: 'item' as const, id: e.label, label: `Copy as ${e.label}`, subtitle: e.code,
      icon: copied === e.label ? Check : Copy, onclick: () => void copy(e.label, e.code),
    })),
  );

  /** Test texts tried before, most recent first, one entry each. */
  const textHistory = $derived.by<DropdownItem[]>(() => {
    const seen = new Set<string>();
    const out: DropdownItem[] = [];
    for (const k of [...bennuRegexStore.recent, ...bennuRegexStore.saved]) {
      if (!k.text.trim() || seen.has(k.text) || k.text === text) continue;
      seen.add(k.text);
      const first = k.text.split('\n')[0];
      out.push({
        kind: 'item', id: `h${out.length}`, label: first.length > 60 ? `${first.slice(0, 60)}…` : first,
        subtitle: k.text.includes('\n') ? `${k.text.split('\n').length} lines` : undefined,
        onclick: () => { text = k.text; },
      });
      if (out.length >= 12) break;
    }
    return out;
  });

  function apply() {
    if (!target) { const first = exportsFor(pattern, flavour, flags)[0]; if (first) void copy(first.label, first.code); return; }
    if (target.apply(pattern)) {
      remember();
      toastStore.show('Pattern written back to the source', 'success');
      onClose();
    } else {
      toastStore.show('The source changed since the tester opened — reopen it from the literal', 'warning');
    }
  }

  function onClosing() { remember(); onClose(); }

  const tabIds = ['matches', 'explain', 'library'];

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); apply(); return; }
    if (e.altKey && !e.ctrlKey && !e.metaKey && e.key >= '1' && e.key <= String(tabIds.length)) {
      e.preventDefault();
      sideTab = tabIds[Number(e.key) - 1];
      return;
    }
    const t = e.target as HTMLElement;
    const typing = t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable;
    if (!typing && !e.ctrlKey && !e.metaKey && !e.altKey && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      patternEl?.focus();
    }
  }

  const tabs = $derived([
    { id: 'matches', label: 'Matches', badge: result ? result.matches.length : undefined },
    { id: 'explain', label: 'Explain' },
    { id: 'library', label: 'Saved', badge: bennuRegexStore.saved.length || undefined },
  ]);

  const engineNote = $derived(
    flavour === 'js' ? 'Browser engine'
      : flavour === 'java' ? 'Java-semantics matcher'
        : 'Arbor matcher · Rust/PCRE syntax',
  );
</script>

<Modal onClose={onClosing} width="1100px" height="700px" padBody={false} ariaLabel="Regex Tester">
  {#snippet header()}
    <ModalHeader onClose={onClosing}>
      <Regex size={14} />
      <span class="modal-title">Regex Tester</span>
      {#if target}<span class="hdr-note">editing a pattern from the source</span>{/if}
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="rx" onkeydown={onKeydown}>
    <div class="rx-top">
      <div class="rx-row">
        <div class="rx-flavour"><Select value={flavour} options={flavourOptions} onchange={(v) => setFlavour(String(v))} size="sm" /></div>
        <div class="rx-pattern">
          <RegexPatternField bind:value={pattern} bind:element={patternEl} {spans} emphasis={patternHover}
            placeholder="Pattern — press F to focus" oncaret={(p) => (caret = p)} />
        </div>
        <RegexInsertMenu oninsert={(t) => void insertToken(t)} />
      </div>
      <div class="rx-row rx-flags">
        {#each FLAGS[flavour] as f (f.key)}
          <ToggleButton label={f.key} title={f.title} pressed={flags.includes(f.key)} onclick={() => toggleFlag(f.key)} />
        {/each}
        {#if flavour !== 'js'}
          <div class="rx-mode"><Select value={mode} options={modeOptions} onchange={(v) => (mode = String(v) as MatchMode)} size="sm" /></div>
        {/if}
        <span class="rx-engine">{engineNote}</span>
      </div>
      {#if syntaxError}
        <Alert variant="error" compact text={syntaxError.message + (parsed.errors.length > 1 ? ` (+${parsed.errors.length - 1} more)` : '')} />
      {:else if outcome?.kind === 'timeout'}
        <Alert variant="warning" compact title="Timed out"
          text="The pattern ran for over {outcome.ms / 1000}s and was stopped — it probably backtracks catastrophically on this text." />
      {:else if outcome?.kind === 'crash'}
        <Alert variant="error" compact text={outcome.message} />
      {:else if result?.error}
        <Alert variant="error" compact text={result.error} />
      {:else if result?.aborted}
        <Alert variant="warning" compact title="Stopped early" text="The matcher hit its step budget; the matches shown are partial." />
      {/if}
    </div>

    <div class="rx-body">
      <div class="rx-main">
        <div class="rx-label">
          <span>Test text</span>
          {#if running && slow}<Spinner size={11} />{/if}
          <span class="rx-grow"></span>
          <span class="rx-count">{text.length} chars</span>
          {#if textHistory.length > 0}
            <Dropdown items={textHistory} width="360px" emptyMessage="No earlier texts.">
              {#snippet trigger({ toggle })}
                <Button variant="ghost" size="xs" onclick={toggle} tooltip="Test texts tried before"><History size={12} /> History</Button>
              {/snippet}
            </Dropdown>
          {/if}
        </div>
        <RegexTestText bind:value={text} {segments} placeholder="Paste or type the text to test against" />

        <div class="rx-row rx-sub">
          <Toggle bind:checked={useReplacement} size="sm" label="Substitution" />
          {#if useReplacement}
            <div class="rx-grow"><Input bind:value={replacement} size="sm" ariaLabel="Replacement"
              placeholder={flavour === 'js' ? '$1 · $<name> · $&' : '$1 · ${name} · \\$'} /></div>
          {/if}
        </div>
        {#if useReplacement}
          {#if result?.replaceError}
            <Alert variant="error" compact text={result.replaceError} />
          {:else if result?.replaced !== null && result?.replaced !== undefined}
            <pre class="rx-out" aria-label="Substitution result">{result.replaced}</pre>
          {/if}
        {/if}
        {#if mode === 'find' && result?.split}
          <div class="rx-label"><span>Split</span></div>
          <pre class="rx-out" aria-label="Split result">{JSON.stringify(result.split)}</pre>
        {/if}
      </div>

      <div class="rx-side">
        <Tabs items={tabs} value={sideTab} variant="underline" size="sm" ariaLabel="Tester views" onSelect={(id) => (sideTab = id)} />
        <div class="rx-pane">
          {#if sideTab === 'matches'}
            <RegexMatchList matches={result?.matches ?? []} groupNames={result?.groupNames ?? [null]}
              truncated={result?.truncated ?? false} ran={result !== null} hasText={text !== ''}
              onhover={(r) => (textHover = r)} />
          {:else if sideTab === 'explain'}
            <RegexExplain rows={explanation} {caret} onhover={(r) => (patternHover = r)} />
          {:else}
            <RegexLibrary current={snapshot()} onPick={load} />
          {/if}
        </div>
        {#if sideTab === 'matches' && result}
          <div class="rx-stat">{result.matches.length} match{result.matches.length === 1 ? '' : 'es'} · {result.ms} ms</div>
        {/if}
      </div>
    </div>
  </div>

  {#snippet footer()}
    <ModalFooter align="end">
      <Dropdown items={copyItems} width="440px" direction="up">
        {#snippet trigger({ toggle })}
          <Button variant="secondary" size="sm" onclick={toggle}><Copy size={13} /> Copy as…</Button>
        {/snippet}
      </Dropdown>
      <Button variant="secondary" size="sm" onclick={onClosing}>Close</Button>
      <Button variant="primary" size="sm" onclick={apply} disabled={!pattern || !!syntaxError}
        tooltip={{ content: target ? 'Write the pattern back into the source' : 'Copy the pattern as code', shortcut: 'Ctrl+Enter' }}>
        {target ? 'Apply to source' : 'Copy'}
      </Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .hdr-note { font-size: var(--font-size-xs); color: var(--text-muted); }

  .rx {
    /* The six tints a capture group is drawn with, in the test text and in the Matches table alike. */
    --rx-g1: var(--warning);
    --rx-g2: var(--success);
    --rx-g3: var(--info, var(--accent));
    --rx-g4: var(--danger);
    --rx-g5: var(--color-tag, #c678dd);
    --rx-g6: var(--accent);
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    container-type: inline-size;
  }
  .rx-top { display: flex; flex-direction: column; gap: 6px; padding: 10px 14px 8px; border-bottom: 1px solid var(--border-subtle); }
  .rx-body { flex: 1; min-height: 0; display: flex; }
  .rx-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 6px; padding: 10px 14px; }
  .rx-side { width: 360px; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0; border-left: 1px solid var(--border-subtle); }
  .rx-pane { flex: 1; min-height: 0; overflow: auto; padding: 8px 8px; }
  .rx-stat { padding: 5px 14px; border-top: 1px solid var(--border-subtle); font-size: var(--font-size-xs); color: var(--text-muted); }

  .rx-row { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .rx-flavour { flex-shrink: 0; }
  .rx-pattern { flex: 1; min-width: 0; }
  .rx-grow { flex: 1; min-width: 0; }
  .rx-flags { flex-wrap: wrap; gap: 4px; }
  .rx-mode { width: 120px; margin-left: 6px; }
  .rx-engine { margin-left: auto; font-size: var(--font-size-xs); color: var(--text-muted); }
  .rx-sub { margin-top: 2px; }
  .rx-label { display: flex; align-items: center; gap: 6px; min-height: 22px; font-size: var(--font-size-xs); font-weight: 600; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.04em; }
  .rx-count { font-weight: 400; text-transform: none; letter-spacing: 0; }
  .rx-out {
    margin: 0;
    padding: 6px 10px;
    max-height: 110px;
    overflow: auto;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
    font-family: var(--font-code);
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  /* Narrow window: the side panel drops under the test text instead of squeezing it. */
  @container (max-width: 760px) {
    .rx-body { flex-direction: column; overflow: auto; }
    .rx-main { flex: none; min-height: 300px; }
    .rx-side { width: auto; border-left: 0; border-top: 1px solid var(--border-subtle); min-height: 240px; }
  }
</style>
