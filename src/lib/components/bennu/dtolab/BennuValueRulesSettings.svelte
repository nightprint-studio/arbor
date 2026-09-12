<script lang="ts">
  /**
   * Settings › Test Values — what the DTO Lab fills a field with, for its name or a constraint it
   * carries: in the valid instance of a generated test, in the invalid case of a constraint Bennu does
   * not know, and in the payload sketch.
   *
   * Two lists because they are owned differently. Yours are edited and ordered — the first that answers
   * wins. The built-ins are only switched off, or copied into one of yours, which then replaces the
   * built-in of its name.
   */
  import { onMount } from 'svelte';
  import { ArrowDown, ArrowUp, Copy, PenLine, Plus, Trash2 } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Card from '$lib/components/shared/ui/Card.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import RowList from '$lib/components/shared/ui/RowList.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuValueRulesStore as values } from '$lib/stores/bennu/value-rules.svelte';
  import type { DtoLabValueRule } from '$lib/ipc/bennu/dtolab';
  import BennuValueRuleModal from './BennuValueRuleModal.svelte';

  onMount(() => { void values.load(); });

  /** The rule in the dialog: `previous` is the name it is saved over, `null` for a new one. */
  let editing = $state<{ rule: DtoLabValueRule | null; previous: string | null } | null>(null);

  const builtinNames = $derived(new Set(values.builtins.map((b) => b.name)));
  const mineNames = $derived(new Set(values.rules.map((r) => r.name)));

  function matchesOf(rule: DtoLabValueRule): string {
    return [...rule.constraints.map((c) => `@${c}`), ...rule.fields].join(', ');
  }
</script>

{#snippet summary(rule: DtoLabValueRule)}
  <span class="vr-text">
    <span class="vr-name">{rule.name}</span>
    <span class="vr-match" use:tooltip={matchesOf(rule)}>{matchesOf(rule)}</span>
  </span>
  <code class="vr-value" use:tooltip={rule.java ? `${rule.java} — checked as ${rule.value}` : rule.value}>{rule.java || rule.value}</code>
{/snippet}

<div class="vr">
  <Card title="Your test values">
    {#snippet actions()}
      <Button variant="ghost" size="xs" onclick={() => (editing = { rule: null, previous: null })}>
        {#snippet iconStart()}<Plus size={12} />{/snippet}
        New value…
      </Button>
    {/snippet}
    <p class="vr-about">
      What a field is given in the DTO Lab's tests and payload sketch, for its name or a constraint it
      carries. Names match ignoring case, <code>_</code> and <code>-</code>, and a <code>*</code> at either
      end matches any prefix or suffix. The first value that answers a field and fits it wins — yours before
      the built-ins, a constraint before a name.
    </p>
    {#if values.rules.length === 0}
      <EmptyState compact message="No test values of your own" description="Start one with New value…, or copy a built-in below to change it." />
    {:else}
      <RowList items={values.rules} key={(r) => r.name}>
        {#snippet row(rule, index)}
          {@render summary(rule)}
          {#if builtinNames.has(rule.name)}<Badge variant="tone" tone="accent" label="Replaces a built-in" />{/if}
          <span class="vr-actions">
            <IconButton tooltip="Try earlier" size={22} disabled={index === 0} onclick={() => void values.move(rule.name, -1)}>
              <ArrowUp size={12} />
            </IconButton>
            <IconButton tooltip="Try later" size={22} disabled={index === values.rules.length - 1} onclick={() => void values.move(rule.name, 1)}>
              <ArrowDown size={12} />
            </IconButton>
            <IconButton tooltip="Edit" size={22} onclick={() => (editing = { rule, previous: rule.name })}>
              <PenLine size={12} />
            </IconButton>
            <IconButton tooltip="Delete" variant="danger" size={22} onclick={() => void values.remove(rule.name)}>
              <Trash2 size={12} />
            </IconButton>
          </span>
        {/snippet}
      </RowList>
    {/if}
    {#if values.path}<p class="vr-dir">Kept in <code>{values.path}</code></p>{/if}
  </Card>

  <Card title="Built-in test values">
    <p class="vr-about">
      Common Italian and English fields. Every value is valid for what it names — the tax code, VAT number,
      IBAN and card number carry the right check digits — and a built-in switched off is simply not tried.
    </p>
    <RowList
      items={values.builtins}
      key={(r) => r.name}
      dimmed={(r) => mineNames.has(r.name) || values.disabled.includes(r.name)}
    >
      {#snippet row(rule)}
        {@render summary(rule)}
        {#if mineNames.has(rule.name)}<Badge variant="tone" tone="neutral" label="Replaced by yours" />{/if}
        <span class="vr-actions rl-keep">
          <IconButton tooltip="Copy into one of yours" size={22} onclick={() => (editing = { rule, previous: null })}>
            <Copy size={12} />
          </IconButton>
          <Toggle
            checked={!values.disabled.includes(rule.name)}
            disabled={mineNames.has(rule.name)}
            ariaLabel={`Use the built-in ${rule.name} value`}
            onchange={(on) => void values.setBuiltinEnabled(rule.name, on)}
          />
        </span>
      {/snippet}
    </RowList>
  </Card>
</div>

{#if editing}
  <BennuValueRuleModal rule={editing.rule} previous={editing.previous} onClose={() => (editing = null)} />
{/if}

<style>
  .vr { display: flex; flex-direction: column; gap: 12px; }
  .vr-about { margin: 0 0 8px; font-size: var(--font-size-sm); color: var(--text-secondary); }
  .vr-about code, .vr-dir code { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary); }
  .vr-text { display: flex; flex-direction: column; min-width: 0; flex: 1; }
  .vr-name { font-family: var(--font-code); font-size: var(--font-size-sm); color: var(--text-primary); }
  .vr-match { font-size: var(--font-size-xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .vr-value {
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--accent);
    max-width: 38%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex-shrink: 1;
  }
  .vr-actions { display: inline-flex; align-items: center; gap: 4px; flex-shrink: 0; }
  .vr-dir { margin: 8px 0 0; font-size: var(--font-size-xs); color: var(--text-muted); overflow-wrap: anywhere; }
</style>
