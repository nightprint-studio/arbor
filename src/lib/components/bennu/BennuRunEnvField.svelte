<script lang="ts">
  /**
   * The environment of a run configuration: variables typed here, plus an optional `.env` file.
   *
   * Extracted because every kind of run configuration has an environment — a JVM launch, a cargo
   * command, a script — and the alternative was the same grid markup once per form. A row that gains
   * an a11y label or a new affordance should gain it everywhere at once.
   *
   * ## Two views of the same variables
   *
   * A **table** for the handful you type by hand, a **text** box for a block pasted from a
   * colleague's README or a `.env.example` — which is why the conversion lives in `env-text.ts`, one
   * definition both views share. Switching is lossless for everything the file format can say;
   * comments are not variables and do not survive the trip.
   *
   * ## The `.env` file is read at launch
   *
   * It is a reference, not an import: nothing from the file is copied into the configuration, the
   * launcher reads it every time, and the rows here win over it. So the file stays the source of
   * truth, and a secret kept in a git-ignored `.env` never reaches the committed run configuration.
   *
   * Rows with an empty key are kept while you type (the launcher drops them; see `envRecord`), so a
   * half-written variable does not vanish under the caret.
   */
  import { Plus, X, Eye, EyeOff, FolderOpen, FileText, Table2, AlignLeft, Variable } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { EnvVar, RunConfig } from '$lib/stores/bennu/run-config.svelte';
  import { envToText, textToEnv, isSecretKey } from './env-text';

  let {
    config,
    patch,
    root,
  }: {
    config: RunConfig;
    patch: (p: Partial<Omit<RunConfig, 'id'>>) => void;
    /** Where the file picker opens when no file is named yet. */
    root: string;
  } = $props();

  const env = $derived(config.env);
  const envFile = $derived(config.envFile);
  /** The whole list, replaced. The store holds configurations immutably, so a patch is a new
   *  array rather than a mutation of this one. */
  const onchange = (next: EnvVar[]) => patch({ env: next });
  const onEnvFile = (path: string) => patch({ envFile: path });

  /** Folded unless there is something in it: an empty environment is the common case and a form
   *  that opens it by default is a form with a block of nothing in the middle. Seeded once. */
  // svelte-ignore state_referenced_locally
  let open = $state(config.env.length > 0 || config.envFile.trim().length > 0);

  const summary = $derived(
    [
      env.filter((e) => e.key.trim()).length ? `${env.filter((e) => e.key.trim()).length} variables` : '',
      envFile.trim() ? (envFile.trim().split(/[\\/]/).pop() ?? '') : '',
    ].filter(Boolean).join(' · ') || 'none',
  );

  let mode = $state<'table' | 'text'>('table');
  let picking = $state(false);
  /** Rows whose value the user chose to show. Keyed by index, which is stable for the lifetime of a
   *  row except across a removal — where showing one extra value for a moment is the safe error. */
  let revealed = $state<Record<number, boolean>>({});

  /** The text view's own buffer. It is seeded when the view opens and then **not** rewritten from
   *  `env`: parsing every keystroke back into rows and re-serialising would drop a half-typed line
   *  (`FOO` has no `=` yet) and throw the caret to the end. */
  let draft = $state('');

  function setMode(next: string) {
    if (next === 'text') draft = envToText(env);
    mode = next === 'text' ? 'text' : 'table';
  }

  function add() {
    onchange([...env, { key: '', value: '' }]);
  }
  function update(idx: number, patch: Partial<EnvVar>) {
    onchange(env.map((e, i) => (i === idx ? { ...e, ...patch } : e)));
  }
  function remove(idx: number) {
    revealed = {};
    onchange(env.filter((_, i) => i !== idx));
  }
  function onDraft(e: Event) {
    draft = (e.currentTarget as HTMLTextAreaElement).value;
    onchange(textToEnv(draft));
  }

  const hidden = (row: EnvVar, i: number) => isSecretKey(row.key) && !revealed[i];

  const dirOf = (p: string) => p.replace(/[\\/][^\\/]*$/, '');
</script>

<FormSection label="Environment" collapsible bind:open {summary}>
  {#snippet icon()}<Variable size={12} />{/snippet}
<div class="env">
  <div class="env-bar">
    <RadioGroup
      value={mode}
      size="sm"
      options={[
        { value: 'table', label: 'Table', icon: Table2 },
        { value: 'text', label: 'Text', icon: AlignLeft },
      ]}
      onchange={setMode}
    />
    <span class="env-count">{env.filter((e) => e.key.trim()).length} variables</span>
    {#if mode === 'table'}
      <button
        class="icon-btn"
        type="button"
        onclick={add}
        use:tooltip={'Add variable'}
        aria-label="Add environment variable"
      >
        <Plus size={13} />
      </button>
    {/if}
  </div>

  {#if mode === 'text'}
    <textarea
      class="env-text"
      rows="7"
      spellcheck="false"
      aria-label="Environment variables as KEY=value lines"
      placeholder={'SPRING_DATASOURCE_URL=jdbc:postgresql://localhost/db\n# comments are ignored'}
      value={draft}
      oninput={onDraft}
    ></textarea>
    <p class="env-hint">One KEY=value per line. Paste a whole .env block to fill the list.</p>
  {:else if env.length === 0}
    <div class="env-empty">No variables. Add one, or point to a .env file below.</div>
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
            type={hidden(row, i) ? 'password' : 'text'}
            placeholder="value"
            ariaLabel="Variable value"
            oninput={(v) => update(i, { value: v })}
          />
          {#if isSecretKey(row.key)}
            <button
              class="icon-btn"
              type="button"
              onclick={() => (revealed[i] = !revealed[i])}
              use:tooltip={revealed[i] ? 'Hide value' : 'Show value'}
              aria-label={revealed[i] ? 'Hide value' : 'Show value'}
            >
              {#if revealed[i]}<EyeOff size={13} />{:else}<Eye size={13} />{/if}
            </button>
          {:else}
            <span class="icon-spacer" aria-hidden="true"></span>
          {/if}
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

  <FormField
    label=".env file"
    hint="Read every time this configuration runs, not copied here. Variables above win over it."
  >
    {#snippet actions()}
      <button class="pick-btn" type="button" onclick={() => (picking = true)}>
        <FolderOpen size={12} /> Browse…
      </button>
    {/snippet}
    <Input
      value={envFile}
      placeholder=".env"
      ariaLabel="Path of the .env file"
      oninput={onEnvFile}
    >
      {#snippet iconStart()}<FileText size={13} />{/snippet}
    </Input>
  </FormField>
</div>
</FormSection>

{#if picking}
  <FileExplorerModal
    mode="file"
    title="Pick a .env file"
    initialPath={envFile.trim() ? dirOf(envFile.trim()) : root}
    onConfirm={(file) => { onEnvFile(file); picking = false; }}
    onCancel={() => (picking = false)}
    onClose={() => (picking = false)}
  />
{/if}

<style>
  .env { display: flex; flex-direction: column; gap: 10px; }
  .env-bar { display: flex; align-items: center; gap: 10px; }
  .env-count { flex: 1; font-size: var(--font-size-xs); color: var(--text-muted); }
  .env-empty { font-size: var(--font-size-xs); color: var(--text-muted); padding: 2px 0; }
  .env-hint { margin: 0; font-size: var(--font-size-2xs); color: var(--text-muted); }
  .env-rows { display: flex; flex-direction: column; gap: 6px; }
  .env-row {
    display: grid;
    grid-template-columns: 1fr auto 1.4fr 24px 24px;
    align-items: center;
    gap: 6px;
  }
  .env-eq { color: var(--text-muted); font-family: var(--font-code); }
  .icon-spacer { width: 24px; }
  .env-text {
    box-sizing: border-box; width: 100%; resize: vertical;
    padding: 8px 10px;
    background: var(--bg-base); color: var(--text-primary);
    border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
    font: var(--font-size-xs) / 1.6 var(--font-code);
  }
  .env-text:focus { outline: none; border-color: var(--border-focus, var(--accent)); }
  .icon-btn {
    display: inline-flex; align-items: center; justify-content: center;
    width: 24px; height: 24px;
    background: transparent; border: none; border-radius: var(--radius-sm);
    color: var(--text-secondary); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .icon-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .icon-btn:disabled { opacity: 0.4; cursor: default; }
  .pick-btn {
    display: inline-flex; align-items: center; gap: 5px;
    height: 24px; padding: 0 8px;
    background: transparent; border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary); cursor: pointer;
    font: var(--font-size-xs) var(--font-ui-sans);
  }
  .pick-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
</style>
