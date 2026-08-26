<script lang="ts">
  /**
   * Path-scoped exceptions to the naming rules — the `[[naming.overrides]]` entries.
   *
   * A project does not have one convention. Test sources are the standing case: names like
   * `test00_invalid_ragioneSociale` mix camelCase and snake_case deliberately, and judging them by
   * the main rule reports hundreds of violations that are not violations of anything.
   *
   * The alternative already existed — put the tree in "Never check" — and it answers a different
   * question: it also stops reporting the names there that really are wrong. An override replaces
   * only the targets it names, so a test tree can free up method names and keep its type and
   * constant rules.
   *
   * ## Only the rules that were set
   *
   * A row here lists the targets the PROJECT configured, not every target that exists: an override
   * of a rule nobody set changes nothing, and offering all twelve would bury the two that matter.
   *
   * ## Order is meaning
   *
   * Later wins, so the list is ordered and each row is addressed by index. That is why removing one
   * is an explicit button rather than "clear the paths".
   */
  import { untrack } from 'svelte';
  import { Plus, Trash2 } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { bennuNamingStore } from '$lib/stores/bennu/naming.svelte';
  import type { NamingConvention, NamingPack, NamingTarget } from '$lib/ipc/bennu/naming';

  interface Props {
    /** Whether the naming check is on — an override of a check nobody runs is not editable. */
    enabled: boolean;
    /** The packs on screen in the rules grid above, in the same order. */
    packs: NamingPack[];
    /** The convention choices, already labelled. */
    conventionOptions: { value: string; label: string }[];
    /** Target id → its human label, from the catalog. */
    targetLabels: Record<string, string>;
  }

  const { enabled, packs, conventionOptions, targetLabels }: Props = $props();

  const overrides = $derived(bennuNamingStore.draft.overrides);

  /** The (pack, target) pairs this override can usefully speak about: the ones the project set. */
  function configuredTargets(): { packId: string; packLabel: string; target: NamingTarget }[] {
    const out: { packId: string; packLabel: string; target: NamingTarget }[] = [];
    for (const pack of packs) {
      const rules = bennuNamingStore.draft.rules[pack.id] ?? {};
      for (const [target, convention] of Object.entries(rules)) {
        if (convention === 'any') continue;
        out.push({ packId: pack.id, packLabel: pack.label, target: target as NamingTarget });
      }
    }
    return out;
  }

  const targets = $derived(configuredTargets());

  /** What applies inside this override: its own choice, else the project-wide one. */
  function conventionAt(index: number, packId: string, target: NamingTarget): NamingConvention {
    const own = overrides[index]?.rules[packId]?.[target];
    return own ?? bennuNamingStore.draft.rules[packId]?.[target] ?? 'any';
  }

  function parseGlobs(text: string): string[] {
    return text.split(',').map((g) => g.trim()).filter((g) => g.length > 0);
  }

  function sameGlobs(a: string[], b: string[]): boolean {
    return a.length === b.length && a.every((g, i) => g === b[i]);
  }

  /**
   * The path text per row, held locally for the same reason the "Never check" field is: the
   * `split → trim → filter → join` round-trip is lossy for anything half-typed, so a derived value
   * would delete the comma the moment it was typed. Re-seeded from the draft only when the two
   * genuinely disagree — on load and on Reset, never while typing.
   */
  let pathText = $state<string[]>(overrides.map((o) => o.paths.join(', ')));
  $effect(() => {
    const fromStore = overrides.map((o) => o.paths);
    untrack(() => {
      if (pathText.length !== fromStore.length) {
        pathText = fromStore.map((p) => p.join(', '));
        return;
      }
      fromStore.forEach((paths, i) => {
        if (!sameGlobs(parseGlobs(pathText[i] ?? ''), paths)) pathText[i] = paths.join(', ');
      });
    });
  });

  function onPathsInput(index: number, text: string) {
    pathText[index] = text;
    bennuNamingStore.setOverridePaths(index, parseGlobs(text));
  }
</script>

<div class="ovr" class:dimmed={!enabled}>
  {#if overrides.length === 0}
    <EmptyState
      message="No exceptions. Add one to give a subtree its own rules — test sources, say, where names are mixed on purpose."
      compact
    />
  {/if}

  {#each overrides as override, i (i)}
    <div class="row">
      <div class="row-head">
        <Input
          value={override.name}
          disabled={!enabled}
          placeholder="name this exception (e.g. tests)"
          oninput={(v) => bennuNamingStore.setOverrideName(i, v)}
          ariaLabel="Exception name"
        />
        <Button
          variant="ghost"
          size="sm"
          disabled={!enabled}
          onclick={() => bennuNamingStore.removeOverride(i)}
          tooltip={{ content: 'Remove this exception' }}
        >
          {#snippet iconStart()}<Trash2 size={12} />{/snippet}
        </Button>
      </div>

      <label class="paths">
        <span class="paths-label">Applies to</span>
        <Input
          value={pathText[i] ?? ''}
          disabled={!enabled}
          placeholder="**/src/test/**"
          oninput={(v) => onPathsInput(i, v)}
        />
      </label>
      {#if override.paths.length === 0}
        <!-- Said plainly: an entry with no path is an unfinished one, not a wildcard, and it is
             the one mistake here that silently does nothing. -->
        <p class="warn">Until this has a path, it applies to nothing.</p>
      {/if}

      {#if targets.length === 0}
        <p class="warn">Set a convention above first — an exception can only relax a rule that exists.</p>
      {:else}
        <ul class="rules">
          {#each targets as t (t.packId + t.target)}
            <li class="rule">
              <span class="rule-label">
                {targetLabels[t.target] ?? t.target}
                {#if packs.length > 1}<span class="rule-pack">{t.packLabel}</span>{/if}
              </span>
              <Select
                value={conventionAt(i, t.packId, t.target)}
                options={conventionOptions}
                disabled={!enabled}
                quiet
                highlight={override.rules[t.packId]?.[t.target] !== undefined}
                ariaLabel={`${t.packLabel} ${targetLabels[t.target] ?? t.target} convention in ${override.name || 'this exception'}`}
                onchange={(v) =>
                  bennuNamingStore.setOverrideConvention(i, t.packId, t.target, v as NamingConvention)}
              />
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/each}

  <div class="add">
    <Button variant="ghost" size="sm" disabled={!enabled} onclick={() => bennuNamingStore.addOverride()}>
      {#snippet iconStart()}<Plus size={12} />{/snippet}
      Add an exception
    </Button>
  </div>
</div>

<style>
  .ovr { display: flex; flex-direction: column; gap: 8px; }
  /* Off is a real state, not a disabled form — the same rule the grid above follows. */
  .ovr.dimmed { opacity: 0.55; }

  .row { border: 1px solid var(--border-subtle); border-radius: var(--radius-md); padding: 8px; display: flex; flex-direction: column; gap: 8px; }
  .row-head { display: flex; align-items: center; gap: 6px; }

  .paths { display: flex; align-items: center; gap: 8px; }
  .paths-label { font-size: 12px; color: var(--text-secondary); white-space: nowrap; }

  .warn { margin: 0; font-size: 11px; color: var(--accent-warning); }

  .rules { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; }
  .rule { display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 26px; }
  .rule-label { display: flex; align-items: baseline; gap: 6px; font-size: 12px; color: var(--text-secondary); }
  .rule-pack { font-size: 10px; color: var(--text-muted); }

  .add { display: flex; justify-content: flex-start; }
</style>
