<script lang="ts">
  /**
   * Naming conventions — the `[naming]` section of the project configuration.
   *
   * A row per declaration kind, a column per language pack, and a convention in each cell. Both
   * axes come from the BE's catalog, so a pack or a target added in Rust appears here with no
   * change to this file.
   *
   * ## Why the grid is quiet
   *
   * Almost every cell reads `any` — the default, for every target, in every project. Twenty
   * bordered boxes all saying the same thing is a wall to read row by row; `quiet` + `highlight`
   * on the ones that differ makes the rules the user actually set the only things drawn as
   * controls. That is exactly the pair `Select` documents them for.
   *
   * ## Keyboard-first
   *
   * Tab reaches the master toggle, then each cell in reading order, then the ignore field. Every
   * Select opens and filters from the keyboard; nothing here needs the mouse. The section owns no
   * Apply button — it edits the store's draft, and the modal that hosts it applies.
   */
  import { untrack } from 'svelte';
  import { CaseSensitive } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import BennuNamingOverrides from './BennuNamingOverrides.svelte';
  import { bennuNamingStore } from '$lib/stores/bennu/naming.svelte';
  import type { NamingConvention, NamingTarget } from '$lib/ipc/bennu/naming';

  const catalog = $derived(bennuNamingStore.catalog);
  const draft = $derived(bennuNamingStore.draft);
  const enabled = $derived(draft.enabled);

  const conventionOptions = $derived(
    (catalog?.conventions ?? []).map((c) => ({
      value: c,
      // The value is its own example, except for the off switch, which needs saying.
      label: c === 'any' ? 'any — no rule' : c,
    })),
  );

  /** Target id → label, so a list of rules elsewhere can name them the same way this grid does. */
  const targetLabels = $derived(
    Object.fromEntries((catalog?.targets ?? []).map((t) => [t.id, t.label])),
  );

  /** The convention set for a cell, defaulting to the off switch. */
  function conventionAt(packId: string, target: NamingTarget): NamingConvention {
    return draft.rules[packId]?.[target] ?? 'any';
  }

  /** Whether this pack has any rule set in the draft. */
  function isConfigured(packId: string): boolean {
    return Object.values(draft.rules[packId] ?? {}).some((c) => c !== 'any');
  }

  /**
   * The packs worth showing.
   *
   * This is a screen about *this project*, so a language the project does not contain is not a
   * question worth asking — a pure-Java tree should not be offered a TypeScript column. A pack that
   * already has rules stays visible whatever the project holds, because a setting you cannot see is
   * a setting you cannot turn off.
   */
  let showAll = $state(false);
  const visiblePacks = $derived(
    (catalog?.packs ?? []).filter((p) => showAll || p.present || isConfigured(p.id)),
  );
  const hiddenCount = $derived((catalog?.packs.length ?? 0) - visiblePacks.length);

  function parseGlobs(text: string): string[] {
    return text.split(',').map((g) => g.trim()).filter((g) => g.length > 0);
  }

  /** Element-wise, not by joining: a glob may contain whatever character a separator would use. */
  function sameGlobs(a: string[], b: string[]): boolean {
    return a.length === b.length && a.every((g, i) => g === b[i]);
  }

  /**
   * The ignore globs, as the comma-separated string the field edits.
   *
   * Held locally rather than derived from the draft: the round-trip through
   * `split → trim → filter → join` is lossy for anything half-typed, so a derived value would
   * delete the comma the moment you typed it. The draft is written on every keystroke; the text is
   * re-seeded from the draft only when the two genuinely disagree — which happens on load and on
   * Reset, and never while typing.
   */
  let ignoreText = $state(bennuNamingStore.draft.ignore.join(', '));
  $effect(() => {
    const fromStore = bennuNamingStore.draft.ignore;
    untrack(() => {
      if (!sameGlobs(parseGlobs(ignoreText), fromStore)) {
        ignoreText = fromStore.join(', ');
      }
    });
  });

  function onIgnoreInput(text: string) {
    ignoreText = text;
    bennuNamingStore.setIgnore(parseGlobs(text));
  }
</script>

<section class="cfg-section">
  <div class="sec-head">
    <CaseSensitive size={13} />
    <h3>Naming conventions</h3>
  </div>

  <FormField
    label="Check declaration names"
    hint="Flags a declaration whose name breaks the convention set below, as a weak warning carrying the name that would satisfy it. Alt+Enter renames to it — straight away for a Java local or parameter, through the rename preview for anything a caller, a framework or a JSP could also be referring to."
  >
    <Toggle
      checked={enabled}
      onchange={(v) => bennuNamingStore.setEnabled(v)}
      label={enabled ? 'On' : 'Off'}
    />
  </FormField>

  {#if !catalog}
    <EmptyState message="Loading the convention catalog…" compact />
  {:else}
    <div class="packs" class:dimmed={!enabled}>
      {#each visiblePacks as pack (pack.id)}
        <div class="pack">
          <div class="pack-head">
            <span class="pack-label">{pack.label}</span>
            <span class="pack-ext">{pack.extensions.map((e) => `.${e}`).join(' ')}</span>
            {#if pack.source === 'symbols'}
              <span
                class="pack-source"
                title="Declarations come from the language server's outline, so locals and parameters are not visible — and the server must be installed for this to check anything"
              >
                via language server
              </span>
            {/if}
            <div class="pack-actions">
              <Button
                variant="ghost"
                size="sm"
                disabled={!enabled}
                onclick={() => bennuNamingStore.adoptStandard(pack.id)}
              >
                Use the standard convention
              </Button>
              <Button
                variant="ghost"
                size="sm"
                disabled={!enabled}
                onclick={() => bennuNamingStore.clearPack(pack.id)}
              >
                Turn all off
              </Button>
            </div>
          </div>
          <ul class="rules">
            {#each catalog.targets as target (target.id)}
              {@const current = conventionAt(pack.id, target.id)}
              {@const supported = pack.supported.includes(target.id)}
              <li class="rule" class:unsupported={!supported}>
                <span class="rule-label">
                  {target.label}
                  {#if !supported}
                    <!-- Not hidden: the row says WHY the rule is unavailable here, which is a fact
                         about the language server, not about the target. Removing it would read as
                         "this kind of declaration does not exist in TypeScript". -->
                    <span class="rule-note" title="A language server's outline lists types and their members only, so Bennu never sees these">
                      not in the outline
                    </span>
                  {:else if current !== 'any' && (pack.source === 'symbols' || !target.fileLocal)}
                    <!-- Stated from both facts, not from the target alone: a declaration an
                         outline reported is reachable from another file whatever kind it is. -->
                    <span class="rule-note" title="Renaming this can reach other files, so its quick-fix opens the rename preview">
                      reaches other files
                    </span>
                  {/if}
                </span>
                <Select
                  value={current}
                  options={conventionOptions}
                  disabled={!enabled || !supported}
                  quiet
                  highlight={supported && current !== 'any'}
                  ariaLabel={`${pack.label} ${target.label} convention`}
                  onchange={(v) =>
                    bennuNamingStore.setConvention(pack.id, target.id, v as NamingConvention)}
                />
              </li>
            {/each}
          </ul>
        </div>
      {/each}
      {#if hiddenCount > 0 || showAll}
        <div class="packs-more">
          <Button variant="ghost" size="sm" onclick={() => (showAll = !showAll)}>
            {showAll
              ? "Show only this project's languages"
              : `Show ${hiddenCount} other language${hiddenCount === 1 ? '' : 's'}`}
          </Button>
        </div>
      {/if}
    </div>

    <FormField
      label="Never check"
      hint="Comma-separated path globs, project-relative (`**/generated/**`, `**/*Stub.java`). Build output and files carrying a generated-code banner are skipped without being listed here."
    >
      <Input
        value={ignoreText}
        disabled={!enabled}
        placeholder="**/generated/**, **/*Stub.java"
        oninput={onIgnoreInput}
      />
    </FormField>

    <FormField
      label="Exceptions"
      hint="A subtree with its own rules. Only the conventions an exception names are replaced — the rest still apply there, which is what separates this from 'Never check'. Later entries win over earlier ones."
    >
      <BennuNamingOverrides
        {enabled}
        packs={visiblePacks}
        {conventionOptions}
        {targetLabels}
      />
    </FormField>
  {/if}
</section>

<style>
  .cfg-section { display: flex; flex-direction: column; gap: 10px; }
  .sec-head { display: flex; align-items: center; gap: 6px; color: var(--text-secondary); }
  .sec-head h3 { margin: 0; font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; }

  .packs { display: flex; flex-direction: column; gap: 12px; }
  /* Off is a real state, not a disabled form: the rules stay readable so a user can set them up
     before switching the check on. */
  .packs.dimmed { opacity: 0.55; }

  .packs-more { display: flex; justify-content: flex-start; }

  .pack { border: 1px solid var(--border-subtle); border-radius: var(--radius-md); overflow: hidden; }
  .pack-head {
    display: flex; align-items: center; gap: 8px;
    padding: 6px 8px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
  }
  .pack-label { font-size: 12px; font-weight: 600; color: var(--text-primary); }
  .pack-ext { font-family: var(--font-mono); font-size: 10px; color: var(--text-muted); }
  .pack-actions { display: flex; gap: 2px; margin-left: auto; }

  .rules { list-style: none; margin: 0; padding: 2px 8px 6px; display: flex; flex-direction: column; }
  .rule {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    min-height: 26px;
  }
  .rule-label { display: flex; align-items: baseline; gap: 6px; font-size: 12px; color: var(--text-secondary); }
  .rule-note { font-size: 10px; color: var(--text-muted); }
  /* Shown, not hidden — the row explains why the rule is unavailable for this language. */
  .rule.unsupported .rule-label { color: var(--text-muted); }

  .pack-source {
    font-size: 10px;
    color: var(--text-muted);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 0 4px;
  }
</style>
