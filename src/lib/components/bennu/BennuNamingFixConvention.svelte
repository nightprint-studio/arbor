<script lang="ts">
  /**
   * Which convention the review is judging by, and the one choice that changes it.
   *
   * A bulk fix rewrites names by a rule, so the rule is the first thing the review states: the
   * project's own (when it switched the check on), what its code already does (kind by kind, with
   * the language's standard where the code does not settle it), or the language's standard outright.
   * Choosing another plans the fix again — the rules decide which names break, so there is nothing to
   * re-filter, only to ask the backend again.
   */
  import { ChevronDown, SlidersHorizontal } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import type { ConventionReport, ConventionSource, ConventionUse } from '$lib/ipc/bennu/naming';
  import { targetIcon } from './naming-target-icons';

  interface Props {
    report: ConventionReport;
    /** Plan again under this convention. */
    onChoose: (source: ConventionSource) => void;
    /** A plan is being built — the choice waits for it. */
    busy?: boolean;
  }

  let { report, onChoose, busy = false }: Props = $props();

  let open = $state(false);

  const options = $derived([
    { value: 'standard', label: 'Standard' },
    { value: 'detected', label: 'Detected' },
    { value: 'configured', label: 'Configured', disabled: !report.configured_available },
  ]);

  const WHAT: Record<ConventionSource, string> = {
    standard: 'the language’s community standard',
    detected: 'how this project already names things, the standard where its code is inconclusive',
    configured: 'the convention this project configured',
  };

  /** The rules of the language the project mostly is: Java first, else the first with rules. */
  const rules = $derived.by((): ConventionUse[] => {
    const java = report.rules.filter((r) => r.pack === 'java');
    return java.length > 0 ? java : report.rules;
  });
  const detectedCount = $derived(rules.filter((r) => r.origin === 'detected').length);

  const ORIGIN_TONE = { configured: 'info', detected: 'success', standard: 'neutral' } as const;
</script>

<div class="conv">
  <div class="conv-row">
    <SlidersHorizontal size={13} />
    <span class="conv-label">Judged by</span>
    <RadioGroup
      value={report.source}
      {options}
      appearance="segment"
      size="sm"
      nowrap
      disabled={busy}
      onchange={(v) => onChoose(v as ConventionSource)}
    />
    <span class="conv-more">
      <Button variant="ghost" size="xs" onclick={() => (open = !open)}>
        Rules <span class="chev" class:open><ChevronDown size={12} /></span>
      </Button>
    </span>
  </div>
  <p class="conv-what">
    Using {WHAT[report.source]}.
    {#if report.source === 'detected'}
      {detectedCount > 0
        ? `${detectedCount} kind${detectedCount === 1 ? '' : 's'} read off the code.`
        : 'Nothing in the code settled a kind, so this is the standard.'}
    {/if}
  </p>

  {#if open}
    <ul class="rules" aria-label="Rules in use">
      {#each rules as r (r.pack + r.target)}
        {@const Icon = targetIcon(r.target)}
        <li class="rule">
          <Icon size={12} />
          <span class="rule-target">{r.target}</span>
          <code class="rule-conv">{r.convention}</code>
          <Badge size="sm" tone={ORIGIN_TONE[r.origin]} variant="tone"
                 tooltip={r.share != null ? `${Math.round(r.share * 100)}% of ${r.votes} names` : undefined}>
            {r.origin}{r.share != null ? ` ${Math.round(r.share * 100)}%` : ''}
          </Badge>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .conv { display: flex; flex-direction: column; gap: 4px; padding: 8px 10px; background: var(--bg-elevated); border-radius: var(--radius-md); }
  .conv-row { display: flex; align-items: center; gap: 8px; color: var(--text-secondary); }
  .conv-label { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-muted); }
  .conv-more { margin-left: auto; }
  .chev { display: inline-flex; transition: transform 120ms; }
  .chev.open { transform: rotate(180deg); }
  .conv-what { margin: 0; font-size: 11px; color: var(--text-muted); }
  .rules { list-style: none; margin: 4px 0 0; padding: 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 2px 14px; }
  .rule { display: flex; align-items: center; gap: 6px; font-size: 11px; color: var(--text-secondary); min-width: 0; }
  .rule-target { min-width: 92px; }
  .rule-conv { font-family: var(--font-mono); color: var(--text-primary); }
</style>
