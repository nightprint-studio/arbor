<script lang="ts">
  /** What the project's validator said about the bound payload, with each message as a user reads it. */
  import { ShieldCheck } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuDtoLabStore as lab } from '$lib/stores/bennu/dtolab.svelte';

  const result = $derived(lab.validateResult);

  function simpleName(fqn: string): string {
    return fqn.split(/[.$]/).pop() ?? fqn;
  }

  function attributes(values: Record<string, string>): string {
    return Object.entries(values).map(([key, value]) => `${key} = ${value}`).join(', ');
  }
</script>

<section class="col">
  <div class="col-head">
    <span class="col-title">Violations</span>
    {#if result && !result.binding_error}
      <span class="count" class:clean={result.violations.length === 0}>{result.violations.length}</span>
    {/if}
    <span class="spacer"></span>
    <div class="locale" use:tooltip={'The locale messages are interpolated in — the JVM default when empty'}>
      <Input bind:value={lab.locale} size="sm" placeholder="Locale — it, en-GB" ariaLabel="Locale for messages" />
    </div>
  </div>
  <div class="col-body">
    {#if lab.validateError}
      <Alert variant="error" compact text={lab.validateError} />
    {:else if !result}
      <EmptyState compact message="Not run yet" description="Ctrl+Enter sends the payload." />
    {:else if result.binding_error}
      <Alert variant="warning" compact title="Nothing was validated: the payload does not bind" text={result.binding_error} />
    {:else}
      {#if result.note}
        <Alert variant="info" compact text={result.note} />
      {/if}
      {#if result.violations.length === 0}
        <div class="valid"><ShieldCheck size={15} /> Valid — no constraint is violated.</div>
      {:else}
        <ul class="violations">
          {#each result.violations as violation, i (i)}
            <li class="violation">
              <div class="top">
                <span class="path">{violation.path || '(the object)'}</span>
                <span class="constraint" use:tooltip={attributes(violation.attributes) || violation.constraint}>@{simpleName(violation.constraint)}</span>
              </div>
              <div class="message">{violation.message}</div>
              {#if violation.template !== violation.message}
                <div class="template">{violation.template}</div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </div>
</section>

<style>
  .col { display: flex; flex-direction: column; min-width: 0; min-height: 0; border-left: 1px solid var(--border-subtle); }
  .col-head {
    display: flex; align-items: center; gap: 6px; flex-shrink: 0;
    min-height: 30px; padding: 3px 8px 3px 10px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .col-title {
    font-size: var(--font-size-2xs); font-weight: 700; letter-spacing: 0.4px; text-transform: uppercase;
    color: var(--text-muted);
  }
  .count {
    font-size: var(--font-size-2xs); font-weight: 700; font-variant-numeric: tabular-nums;
    padding: 0 6px; border-radius: 999px;
    color: var(--error); background: color-mix(in srgb, var(--error) 14%, transparent);
  }
  .count.clean { color: var(--success); background: color-mix(in srgb, var(--success) 14%, transparent); }
  .spacer { flex: 1; }
  .locale { width: 150px; }
  .col-body { flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 6px; padding: 6px 8px; overflow: auto; }
  .valid { display: flex; align-items: center; gap: 6px; padding: 6px 2px; font-size: var(--font-size-sm); color: var(--success); }
  .violations { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .violation { padding: 5px 6px; border-radius: var(--radius-sm); border-left: 2px solid var(--error); background: var(--bg-overlay); }
  .top { display: flex; align-items: baseline; gap: 8px; }
  .path { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); }
  .constraint { margin-left: auto; font-size: var(--font-size-2xs); color: var(--text-muted); white-space: nowrap; }
  .message { font-size: var(--font-size-xs); color: var(--text-secondary); margin-top: 1px; }
  .template { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); margin-top: 1px; overflow-wrap: anywhere; }
</style>
