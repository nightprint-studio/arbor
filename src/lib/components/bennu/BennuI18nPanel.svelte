<script lang="ts">
  /**
   * The i18n tool window — the translation under the caret, rendered, with its parameters below it.
   *
   * ## A split beside the editor
   *
   * It sits on the right, sharing the row with the editor, because the two are read **together**: the
   * markup on the left, what it comes out as on the right, and one keystroke changing both. That is a
   * split pane rather than a console — nothing here is a log you consult after the fact.
   *
   * The consequence is a narrow column, and the layout is built around it: the sentence takes the full
   * width at the top and the parameter table goes underneath rather than beside it. A translation is
   * the one thing here whose meaning depends on how it wraps, so it is the one thing that never shares
   * a row.
   *
   * ## Why it follows the caret rather than having its own selection
   *
   * There is no "current label" for the panel to own. You are editing a file, the caret is on a line,
   * and that line is the translation — anything else would be a second cursor to keep in step with the
   * first. So the panel has no navigation of its own except the one that goes *outward*: the language
   * picker, which is what stops the loop of translating (read the Italian, write the English, check the
   * Italian) from being four clicks through the project tree each time round.
   */
  import { Languages, Quote, RefreshCw, TriangleAlert, X } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuI18nStore } from '$lib/stores/bennu/i18n.svelte';
  import { extRefresh } from '$lib/ipc/bennu/ext';
  import { isI18nBundle } from './i18n/bundle-path';
  import I18nLangPicker from './i18n/I18nLangPicker.svelte';
  import I18nMarkup from './i18n/I18nMarkup.svelte';
  import I18nParamTable from './i18n/I18nParamTable.svelte';
  import I18nToolbar from './i18n/I18nToolbar.svelte';
  import { StyleSheet } from './i18n/markup-style';
  import type { GlossaryDecl, Sibling } from '$lib/ipc/bennu/i18n';

  /** With no value in view there are no samples. Shared, so the derived below is stable. */
  const NO_SAMPLES: ReadonlyMap<string, string> = new Map();

  const view = $derived(bennuI18nStore.view);
  const onBundle = $derived(isI18nBundle(projectStore.activeFilePath));

  /** Resolved once per view: the size scale depends on every declared size (see `markup-style`). */
  const sheet = $derived(new StyleSheet(view?.styles ?? []));
  const glossary = $derived(
    new Map((view?.glossary ?? []).map((g) => [g.key, g] as [string, GlossaryDecl])),
  );
  const samples = $derived(view ? bennuI18nStore.samplesFor(view.label) : NO_SAMPLES);

  /**
   * Whether the toolbar can write into the value.
   *
   * `content_start` is null for a basic string carrying a backslash escape, whose content is shorter
   * than its source — every offset into it drifts past the escape. The banner below says so, because
   * greyed-out buttons with no reason given are indistinguishable from a broken panel, and the fix is
   * one the user can actually apply.
   */
  const writable = $derived(view?.content_start != null);

  /** Parameters the other languages pass and this value does not — the panel's headline number. */
  const owed = $derived.by(() => {
    if (!view) return 0;
    const others = new Set(view.siblings.flatMap((s) => s.params));
    return [...others].filter((p) => !view.params.includes(p)).length;
  });

  /**
   * Why there is nothing to show — a fact, not an instruction that might be wrong.
   *
   * The order is the order the links fail in: a project with no model cannot have translations, and a
   * file with no translations cannot have one under the caret.
   */
  const emptyReason = $derived.by(() => {
    const a = bennuI18nStore.answer;
    if (!a) return 'Put the caret on a translation.';
    if (!a.project) {
      return 'No open project owns this file, so Bennu has nothing to read the languages, the stylesheet or the other translations from.';
    }
    if (!a.model) {
      // The staleness is the likely cause and it is worth naming, because the fix is one button away
      // and the wording decides whether the user finds it.
      return 'Bennu found no i18n/languages.toml in this project when it scanned it. Capabilities are read once per project, so a bundle tree added since then is not seen yet — rescan and it will be. If the tree really has no languages.toml, that file is what declares the languages, and the panel needs it.';
    }
    if (!a.bundle) {
      return 'This file is not a translation bundle. Only i18n/<language>/<category>.toml holds translations.';
    }
    if (a.translations === 0) {
      return 'Bennu sees no translations in this file — only table headers and blank lines. If it plainly has some, that is a parsing problem rather than a caret problem.';
    }
    const n = a.translations;
    return `Put the caret on one of this file's ${n} ${n === 1 ? 'translation' : 'translations'}.`;
  });

  /** Whether a rescan could plausibly change the answer — see `StudioAnswer.model`. */
  const canRescan = $derived.by(() => {
    const a = bennuI18nStore.answer;
    return !!a && !a.model && !!a.root;
  });
  let rescanning = $state(false);

  /**
   * Rebuild the project's framework model, then ask again.
   *
   * Offered here rather than only from a menu because this is where the problem is noticed, and a
   * diagnosis with no action beside it is a dead end. `extRefresh` drops the project's cached slot, so
   * capabilities are detected afresh — which is the whole point: the tree exists now and did not when
   * the project was opened.
   */
  async function rescan() {
    const root = bennuI18nStore.answer?.root;
    if (!root) return;
    rescanning = true;
    try {
      await extRefresh(root);
      bennuI18nStore.requestRetry();
    } catch {
      // Nothing to add: the panel will re-ask and say whatever is true then.
    } finally {
      rescanning = false;
    }
  }

  /** The languages that have this label — what the comparison list shows. */
  const translated = $derived((view?.siblings ?? []).filter((s) => s.declares));

  /**
   * Open another language's file, landing on the value.
   *
   * A language that does not declare the label has no value to land on, and possibly no file either.
   * Opening it is still the right move — that is where the translation goes — so the offset is simply
   * not used, and a file that does not exist yet fails the way every other missing-file open does.
   */
  function goToLang(s: Sibling) {
    void projectStore
      .openFile(s.file)
      .then(() => { if (s.declares) bennuUiStore.requestGotoOffset(s.offset); })
      .catch(() => { /* not written yet — the picker already said so */ });
  }
</script>

<PanelShell title="i18n">
  {#snippet icon()}<Languages size={13} />{/snippet}

  {#snippet actions()}
    {#if view}
      <I18nToolbar {view} {writable} onInsert={(what) => bennuI18nStore.insert(what)} />
    {/if}
    <IconButton tooltip="Close panel" size={22} onclick={() => bennuUiStore.toggleRight('i18n')}>
      <X size={12} />
    </IconButton>
  {/snippet}

  <!-- Declared unconditionally and gated inside: a snippet is a prop, and a prop wrapped in an
       `{#if}` is a prop the component may never be handed. -->
  {#snippet toolbar()}
    {#if view}
      <div class="i18n-bar">
        <I18nLangPicker {view} onGo={goToLang} />
        <span class="i18n-label" use:tooltip={`${view.category} · line ${view.line}`}>
          {view.label}
        </span>
        {#if !view.known}
          <span
            class="i18n-tag new"
            use:tooltip={'Not in the index yet — it will be after the next scan'}>new</span
          >
        {/if}
        {#if view.missing.length > 0}
          <span
            class="i18n-tag owed"
            use:tooltip={`Not declared in ${view.missing.join(', ')}`}>−{view.missing.length}</span
          >
        {/if}
        {#if owed > 0}
          <span
            class="i18n-tag owed"
            use:tooltip={`${owed} ${owed === 1 ? 'parameter' : 'parameters'} the other languages pass and ${view.lang} does not`}
            >{owed} unused</span
          >
        {/if}
      </div>
    {/if}
  {/snippet}

  {#if !onBundle}
    <div class="i18n-empty">
      <EmptyState
        message="Open a translation file — i18n/<language>/<category>.toml — and the value under the caret appears here."
      />
    </div>
  {:else if bennuI18nStore.failed}
    <div class="i18n-empty">
      <EmptyState message="Bennu could not read the bundle. The backend may still be starting." />
    </div>
  {:else if !view}
    <div class="i18n-empty">
      {#if bennuI18nStore.loading}
        <Spinner />
      {:else}
        <!-- Which of these it is matters: only the last is normal, and saying "put the caret on a
             translation" for the other two is telling somebody to do something they have already
             done. See `StudioAnswer`. -->
        <div class="i18n-reason">
          <EmptyState message={emptyReason} />
          {#if canRescan}
            <Button size="sm" variant="secondary" loading={rescanning} onclick={rescan}>
              <RefreshCw size={12} /> Rescan the project
            </Button>
          {/if}
        </div>
      {/if}
    </div>
  {:else}
    <div class="i18n-body">
      {#if !writable}
        <!-- Actionable, so it says the fix rather than the rule. -->
        <p class="i18n-warn">
          <Quote size={12} />
          <span>
            A double-quoted string with an escape in it: Bennu cannot point at a position inside one.
            Rewrite it with <code>'single quotes'</code> and the toolbar, the colouring and the
            problem markers all start working — which is what the markup wants anyway, since
            <code>"\$"</code> is not a valid TOML escape.
          </span>
        </p>
      {/if}

      <!-- The sentence, at the full width of the panel. -->
      <div class="i18n-render">
        <I18nMarkup segments={view.segments} {sheet} {glossary} {samples} />
      </div>

      {#if view.problems.length > 0}
        <ul class="i18n-problems">
          {#each view.problems as p, i (i)}
            <li>
              <TriangleAlert size={11} />
              <span>{p.message}</span>
              <!-- The offset is into the value, which is the frame a reader can act on: it is
                   "byte 14 of this string", not a position in the file. -->
              <code>at {p.start}</code>
            </li>
          {/each}
        </ul>
      {/if}

      <div class="i18n-sect">Parameters</div>
      <I18nParamTable
        {view}
        {samples}
        onSample={(param, value) => bennuI18nStore.setSample(view.label, param, value)}
        onInsert={(param) => bennuI18nStore.insert({ what: 'placeholder', name: param })}
      />

      {#if translated.length > 0}
        <div class="i18n-sect">Other languages</div>
        <ul class="i18n-sibs">
          {#each translated as s (s.lang)}
            <li>
              <!-- The same destination the picker offers. Both exist because they answer different
                   questions: the picker is "take me to another language", this is "what does the
                   English actually say" — and that one wants the text on screen, not in a menu. -->
              <button
                class="i18n-sib"
                type="button"
                use:tooltip={`Open ${s.file}`}
                onclick={() => goToLang(s)}
              >
                <span class="i18n-sib-lang">{s.lang}</span>
                <span class="i18n-sib-text">{s.text}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}
</PanelShell>

<style>
  /* ── the header's second row ─────────────────────────────────────────────── */
  .i18n-bar {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    padding: 3px 6px;
  }
  .i18n-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-secondary);
  }
  .i18n-tag {
    flex: none;
    padding: 0 4px;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-3xs);
  }
  .i18n-tag.new {
    background: color-mix(in srgb, var(--info) 16%, transparent);
    color: var(--info);
  }
  .i18n-tag.owed {
    background: color-mix(in srgb, var(--warning) 16%, transparent);
    color: var(--warning);
  }

  .i18n-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    padding: 12px;
  }
  /* Message above, the way out below it. */
  .i18n-reason {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
  }

  .i18n-body { padding: 8px 10px 12px; }

  .i18n-warn {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin: 0 0 8px;
    padding: 6px 8px;
    border: 1px solid color-mix(in srgb, var(--warning) 40%, transparent);
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--warning) 8%, transparent);
    font-size: var(--font-size-2xs);
    line-height: 1.5;
    color: var(--text-secondary);
  }
  .i18n-warn :global(svg) { flex: none; margin-top: 2px; color: var(--warning); }
  .i18n-warn code { font-family: var(--font-code); color: var(--text-primary); }

  /* `pre-wrap` is load-bearing: the translation's own spaces are the sentence's spaces, and the
     renderer is written around it — see `I18nMarkup`. */
  .i18n-render {
    padding: 9px 11px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
    color: var(--text-primary);
    font-size: var(--font-size-sm);
    line-height: 1.65;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .i18n-problems {
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .i18n-problems li {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: var(--font-size-2xs);
    color: var(--text-secondary);
  }
  .i18n-problems :global(svg) { flex: none; color: var(--warning); }
  .i18n-problems code {
    margin-left: auto;
    font-family: var(--font-code);
    font-size: var(--font-size-3xs);
    color: var(--text-disabled);
  }

  .i18n-sect {
    margin: 14px 0 4px;
    font-size: var(--font-size-3xs);
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-disabled);
  }

  .i18n-sibs {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .i18n-sib {
    display: flex;
    align-items: baseline;
    gap: 6px;
    width: 100%;
    padding: 3px 4px;
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-secondary);
    font-size: var(--font-size-2xs);
    text-align: left;
    cursor: pointer;
  }
  .i18n-sib:hover { background: var(--bg-hover); color: var(--text-primary); }
  .i18n-sib-lang {
    flex: none;
    padding: 0 4px;
    border-radius: var(--radius-sm);
    background: var(--bg-hover);
    color: var(--text-muted);
    font-family: var(--font-code);
    font-size: var(--font-size-3xs);
  }
  /* Wraps rather than truncating: in a narrow panel a one-line clip shows four words of a sentence,
     which is not enough to compare against the one above it. */
  .i18n-sib-text { flex: 1; min-width: 0; line-height: 1.45; }
</style>
