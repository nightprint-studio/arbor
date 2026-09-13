<script lang="ts">
  /**
   * Naming conventions — one grid, two levels.
   *
   * A row per declaration kind, a column per language pack, and a convention in each cell. Both
   * axes come from the BE's catalog, so a pack or a target added in Rust appears here with no
   * change to this file.
   *
   * ## The same screen for your defaults and for a project
   *
   * The document is a prop, not a lookup, so this file never knows which level it is on:
   * Settings › Editor › Naming hands it the profile, Project Configuration hands it the project
   * plus `inherited` — the profile's loaded copy. With `inherited` present a cell the project has
   * not set shows the *profile's* answer, marked as such, and setting it makes it the project's.
   * That is the whole difference, and it is why there is no second copy of this grid.
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
   * Select opens and filters from the keyboard; nothing here needs the mouse. The screen owns no
   * Apply button — it edits the document's draft, and its host applies.
   */
  import { untrack } from 'svelte';
  import { CaseSensitive, Filter, Layers, SlidersHorizontal } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import BennuNamingOverrides from './BennuNamingOverrides.svelte';
  import { bennuNamingStore, type NamingDocument } from '$lib/stores/bennu/naming.svelte';
  import type { NamingConfig, NamingConvention, NamingTarget } from '$lib/ipc/bennu/naming';

  interface Props {
    /** The document this screen edits. */
    doc: NamingDocument;
    /**
     * The level above, when there is one — the profile's loaded config, on the project screen.
     * `null` on the profile screen itself, which has nothing above it.
     */
    inherited?: NamingConfig | null;
    /** What the level above is called, where a row has to say a value came from it. */
    inheritedLabel?: string;
  }

  const { doc, inherited = null, inheritedLabel = 'your profile' }: Props = $props();

  const catalog = $derived(bennuNamingStore.catalog);
  const draft = $derived(doc.draft);
  /** Whether inherited values are in play at all: there is a level above, and this one takes it. */
  const inheriting = $derived(!!inherited && draft.inherit);
  /** On is either level's word, so the grid is live the moment the profile says so. */
  const enabled = $derived(draft.enabled || (inheriting && !!inherited?.enabled));
  /** Switched on from above rather than here — the row says so instead of looking un-set. */
  const enabledAbove = $derived(!draft.enabled && enabled);

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

  /** Whether this document states a convention for a cell itself — as opposed to taking one. */
  function isOwn(packId: string, target: NamingTarget): boolean {
    return draft.rules[packId]?.[target] !== undefined;
  }

  /** The convention that applies to a cell: this document's, else the level above's, else off. */
  function conventionAt(packId: string, target: NamingTarget): NamingConvention {
    const own = draft.rules[packId]?.[target];
    if (own !== undefined) return own;
    return (inheriting ? inherited?.rules[packId]?.[target] : undefined) ?? 'any';
  }

  /** Whether a cell is showing a value it did not set — what earns the "from …" note. */
  function isInherited(packId: string, target: NamingTarget): boolean {
    return !isOwn(packId, target) && conventionAt(packId, target) !== 'any';
  }

  /** Whether this pack has any rule set at either level. */
  function isConfigured(packId: string): boolean {
    const own = Object.values(draft.rules[packId] ?? {}).some((c) => c !== 'any');
    const above = inheriting && Object.values(inherited?.rules[packId] ?? {}).some((c) => c !== 'any');
    return own || above;
  }

  /** Whether this document states anything of its own about a pack — what "Use …'s" undoes. */
  const statesOwnRules = (packId: string) => Object.keys(draft.rules[packId] ?? {}).length > 0;

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
  // svelte-ignore state_referenced_locally
  let ignoreText = $state(doc.draft.ignore.join(', '));
  $effect(() => {
    const fromStore = doc.draft.ignore;
    untrack(() => {
      if (!sameGlobs(parseGlobs(ignoreText), fromStore)) {
        ignoreText = fromStore.join(', ');
      }
    });
  });

  function onIgnoreInput(text: string) {
    ignoreText = text;
    doc.setIgnore(parseGlobs(text));
  }

  /** The globs the level above contributes — added to these, never replaced by them. */
  const inheritedIgnore = $derived(inheriting ? (inherited?.ignore ?? []) : []);
</script>

<div class="card">
  <div class="card-section-title"><CaseSensitive size={12} /> The check</div>
  <FormRow
    label="Check declaration names"
    description="Flags a declaration whose name breaks the convention set below, as a weak warning carrying the name that would satisfy it. Alt+Enter renames to it — straight away for a Java local or parameter, through the rename preview for anything a caller, a framework or a JSP could also be referring to."
  >
    <Toggle
      checked={draft.enabled}
      onchange={(v) => doc.setEnabled(v)}
      ariaLabel="Check declaration names"
      label={enabledAbove ? `On — from ${inheritedLabel}` : draft.enabled ? 'On' : 'Off'}
    />
  </FormRow>
  {#if inherited}
    <FormRow
      label="Start from {inheritedLabel}"
      description="On, this project states only what it spells differently and takes the rest from your profile. Off, it is judged by itself alone — how a legacy tree escapes conventions every other project of yours has adopted."
    >
      <Toggle
        checked={draft.inherit}
        onchange={(v) => doc.setInherit(v)}
        ariaLabel="Start from your profile"
        label={draft.inherit ? 'Inheriting' : 'This project only'}
      />
    </FormRow>
  {/if}
</div>

{#if !catalog}
  <EmptyState message="Loading the convention catalog…" compact />
{:else}
  <div class="card">
    <div class="card-section-title"><Layers size={12} /> Conventions</div>
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
                onclick={() => doc.adoptRules(pack.id, bennuNamingStore.standardOf(pack.id))}
              >
                Use the standard convention
              </Button>
              {#if inheriting && statesOwnRules(pack.id)}
                <!-- Distinct from "Turn all off", which STATES "no rule" and therefore overrides
                     the profile. This drops the project's answer so the profile's comes back. -->
                <Button variant="ghost" size="sm" onclick={() => doc.unsetPack(pack.id)}>
                  Back to {inheritedLabel}
                </Button>
              {/if}
              <Button
                variant="ghost"
                size="sm"
                disabled={!enabled}
                onclick={() => doc.clearPack(pack.id)}
              >
                Turn all off
              </Button>
            </div>
          </div>
          <ul class="rules">
            {#each catalog.targets as target (target.id)}
              {@const current = conventionAt(pack.id, target.id)}
              {@const supported = pack.supported.includes(target.id)}
              {@const fromAbove = isInherited(pack.id, target.id)}
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
                  {:else if fromAbove}
                    <span class="rule-note inherited" title="This project states nothing here, so your profile answers. Choosing a convention makes it this project's.">
                      from {inheritedLabel}
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
                  highlight={supported && current !== 'any' && !fromAbove}
                  ariaLabel={`${pack.label} ${target.label} convention`}
                  onchange={(v) => doc.setConvention(pack.id, target.id, v as NamingConvention)}
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
  </div>

  <div class="card">
    <div class="card-section-title"><Filter size={12} /> Where it does not apply</div>
    <div class="pad">
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
      {#if inheritedIgnore.length}
        <!-- Added to, never replaced: both levels are naming a place that should not be judged,
             and an intersection would judge it. -->
        <p class="set-hint">
          Also skipped, from {inheritedLabel}: <code>{inheritedIgnore.join(', ')}</code>
        </p>
      {/if}
    </div>
  </div>

  <div class="card">
    <div class="card-section-title"><SlidersHorizontal size={12} /> Exceptions</div>
    <div class="pad">
      <FormField
        label="Subtrees with their own rules"
        hint="Only the conventions an exception names are replaced — the rest still apply there, which is what separates this from 'Never check'. Later entries win over earlier ones."
      >
        <BennuNamingOverrides
          {doc}
          {enabled}
          packs={visiblePacks}
          {conventionOptions}
          {targetLabels}
        />
      </FormField>
    </div>
  </div>
{/if}

<style>
  .pad { padding: 10px 14px 12px; }

  .packs { display: flex; flex-direction: column; gap: 12px; padding: 10px 14px 12px; }
  /* Off is a real state, not a disabled form: the rules stay readable so a user can set them up
     before switching the check on. */
  .packs.dimmed { opacity: 0.55; }

  .packs-more { display: flex; justify-content: flex-start; }

  .pack { border: 1px solid var(--border-subtle); border-radius: var(--radius-md); overflow: hidden; }
  .pack-head {
    display: flex; align-items: center; gap: 8px;
    padding: 6px 8px;
    background: var(--bg-overlay);
    border-bottom: 1px solid var(--border-subtle);
  }
  .pack-label { font-size: 12px; font-weight: 600; color: var(--text-primary); }
  .pack-ext { font-family: var(--font-code); font-size: 10px; color: var(--text-muted); }
  .pack-actions { display: flex; gap: 2px; margin-left: auto; }

  .rules { list-style: none; margin: 0; padding: 2px 8px 6px; display: flex; flex-direction: column; }
  .rule {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    min-height: 26px;
  }
  .rule-label { display: flex; align-items: baseline; gap: 6px; font-size: 12px; color: var(--text-secondary); }
  .rule-note { font-size: 10px; color: var(--text-muted); }
  /* A value that came from the level above reads as borrowed, not as unset. */
  .rule-note.inherited { color: var(--info); }
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
