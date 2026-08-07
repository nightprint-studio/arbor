<script lang="ts">
  /**
   * The environment-variable rows of a run configuration.
   *
   * Extracted because every kind of run configuration has an environment — a JVM launch, a cargo
   * command, and whatever comes next — and the alternative was the same forty lines of grid markup
   * once per form. A row that gains a paste-a-`.env`-file affordance, or an a11y label, should gain
   * it everywhere at once.
   *
   * Rows with an empty key are kept while you type (the launcher drops them; see `envRecord`), so a
   * half-written variable does not vanish under the caret.
   */
  import { Plus, X } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { EnvVar } from '$lib/stores/bennu/run-config.svelte';

  let {
    env,
    onchange,
  }: {
    env: EnvVar[];
    /** The whole list, replaced. The store holds configurations immutably, so a patch is a new
     *  array rather than a mutation of this one. */
    onchange: (next: EnvVar[]) => void;
  } = $props();

  function add() {
    onchange([...env, { key: '', value: '' }]);
  }

  function update(idx: number, patch: Partial<EnvVar>) {
    onchange(env.map((e, i) => (i === idx ? { ...e, ...patch } : e)));
  }

  function remove(idx: number) {
    onchange(env.filter((_, i) => i !== idx));
  }
</script>

<FormField label="Environment variables">
  {#snippet actions()}
    <button
      class="icon-btn"
      type="button"
      onclick={add}
      use:tooltip={'Add variable'}
      aria-label="Add environment variable"
    >
      <Plus size={13} />
    </button>
  {/snippet}
  {#if env.length === 0}
    <div class="env-empty">No environment variables.</div>
  {:else}
    <div class="env-rows">
      {#each env as row, i (i)}
        <div class="env-row">
          <Input
            value={row.key}
            placeholder="NAME"
            ariaLabel="Variable name"
            oninput={(v) => update(i, { key: v })}
          />
          <span class="env-eq">=</span>
          <Input
            value={row.value}
            placeholder="value"
            ariaLabel="Variable value"
            oninput={(v) => update(i, { value: v })}
          />
          <button
            class="icon-btn"
            type="button"
            onclick={() => remove(i)}
            use:tooltip={'Remove'}
            aria-label="Remove variable"
          >
            <X size={13} />
          </button>
        </div>
      {/each}
    </div>
  {/if}
</FormField>

<style>
  .env-empty { font-size: var(--font-size-xs); color: var(--text-muted); padding: 2px 0; }
  .env-rows { display: flex; flex-direction: column; gap: 6px; }
  .env-row {
    display: grid;
    grid-template-columns: 1fr auto 1.4fr auto;
    align-items: center;
    gap: 6px;
  }
  .env-eq { color: var(--text-muted); font-family: var(--font-code); }
  .icon-btn {
    display: inline-flex; align-items: center; justify-content: center;
    width: 24px; height: 24px;
    background: transparent; border: none; border-radius: var(--radius-sm);
    color: var(--text-secondary); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .icon-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .icon-btn:disabled { opacity: 0.4; cursor: default; }
</style>
