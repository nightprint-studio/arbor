<script lang="ts">
  /**
   * The `cargo` arm of the run-configuration editor.
   *
   * Its own file rather than a third branch inside `BennuRunConfigModal`, which is already the
   * largest component in Bennu: a cargo configuration shares almost nothing with a JVM one below the
   * name field, so an inline arm would have been a hundred lines of markup wrapped in an `{#if}`.
   *
   * ## Everything here is picked, not typed
   *
   * The workspace is already known — its crates, what each builds, and the features each declares —
   * so the crate, the command and the target are selects and the features are checkboxes. The only
   * free-text fields are the two argument strings and the working directory, which genuinely cannot
   * be enumerated. A typo in a crate name produces `error: package ID specification 'xy' did not
   * match any packages`, which is a worse experience than not being able to type it.
   *
   * ## The two argument fields are not interchangeable
   *
   * `cargoArgs` are cargo's own flags and go BEFORE the `--`; `programArgs` reach the program or the
   * test harness and go after it. Handing `--nocapture` to cargo is the mistake this separation
   * exists to prevent, and the hints say so.
   */
  import { ChevronDown, Terminal, TriangleAlert } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import BennuRunEnvField from './BennuRunEnvField.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { cargoPreview, hasComponent } from '$lib/ipc/bennu/cargo';
  import { bennuCargoStore } from '$lib/stores/bennu/cargo.svelte';
  import { cargoInvocationOf } from '$lib/stores/bennu/run-config.svelte';
  import type { EnvVar, RunConfig } from '$lib/stores/bennu/run-config.svelte';

  let {
    config,
    patch,
  }: {
    config: RunConfig;
    patch: (p: Partial<Omit<RunConfig, 'id'>>) => void;
  } = $props();

  const workspace = $derived(bennuCargoStore.workspace);
  const commands = $derived(bennuCargoStore.commands);
  const toolchain = $derived(bennuCargoStore.toolchain);

  /** The command's definition, when the backend told us about it. */
  const def = $derived(commands.find((c) => c.id === config.cargoCommand) ?? null);

  /** The crate `-p` names, when the configuration picks one. */
  const crate = $derived(
    workspace?.crates.find((c) => c.name === config.module.trim()) ?? null,
  );

  /** Every command, with the ones needing a missing component marked rather than hidden. */
  const commandOptions = $derived(
    commands.map((c) => ({
      value: c.id,
      label: hasComponent(toolchain, c.component)
        ? c.label
        : `${c.label} — needs ${c.component}`,
    })),
  );

  /** The crates, plus the whole workspace. */
  const crateOptions = $derived([
    { value: '', label: workspace?.is_workspace ? 'The whole workspace' : 'This crate' },
    ...(workspace?.crates ?? []).map((c) => ({
      value: c.name,
      label: c.rel_path ? `${c.name} — ${c.rel_path}` : c.name,
    })),
  ]);

  /**
   * The target selector kinds worth offering for this command.
   *
   * `cargo run` takes a binary or an example and nothing else; everything else can also be aimed at
   * the library or at every target. Offering `--bin` on a `cargo doc` would be offering a flag that
   * the backend then drops.
   */
  const targetKindOptions = $derived.by(() => {
    const base = [{ value: '', label: 'Whatever the command builds' }];
    if (config.cargoCommand === 'run') {
      return [...base, { value: 'bin', label: 'A binary' }, { value: 'example', label: 'An example' }];
    }
    return [
      ...base,
      { value: 'lib', label: 'The library' },
      { value: 'bin', label: 'A binary' },
      { value: 'example', label: 'An example' },
      { value: 'test', label: 'An integration test' },
      { value: 'bench', label: 'A benchmark' },
      { value: 'all-targets', label: 'Every target' },
    ];
  });

  /** The named targets of the chosen kind — across the workspace when no crate is picked, so the
   *  picker is useful before you have narrowed down. */
  const targetOptions = $derived.by(() => {
    const kind = config.cargoTargetKind;
    if (!kind || ['lib', 'all-targets'].includes(kind)) return [];
    const crates = crate ? [crate] : (workspace?.crates ?? []);
    const names = crates.flatMap((c) =>
      c.targets.filter((t) => t.kind === kind).map((t) => t.name),
    );
    return [...new Set(names)].sort().map((n) => ({ value: n, label: n }));
  });

  /** The features the chosen crate declares, as a menu that toggles them on and off. */
  const featureItems = $derived<DropdownItem[]>(
    (crate?.features ?? []).map((f) => ({
      kind: 'item',
      id: f.name,
      label: f.name,
      meta: f.default ? 'default' : undefined,
      active: activeFeatures.includes(f.name),
      onclick: () => toggleFeature(f.name),
    })),
  );

  const activeFeatures = $derived(
    config.cargoFeatures.split(',').map((f) => f.trim()).filter(Boolean),
  );

  function toggleFeature(name: string) {
    const next = activeFeatures.includes(name)
      ? activeFeatures.filter((f) => f !== name)
      : [...activeFeatures, name];
    patch({ cargoFeatures: next.join(',') });
  }

  /** Whether the command this configuration runs needs a component the toolchain does not have. */
  const missingComponent = $derived(
    def && !hasComponent(toolchain, def.component) ? def.component : '',
  );

  /** A named target with no name yet — the one shape the backend drops rather than passes on, so it
   *  is worth saying instead of letting the run quietly widen. */
  const targetIncomplete = $derived(
    ['bin', 'example', 'test', 'bench'].includes(config.cargoTargetKind)
      && !config.cargoTarget.trim(),
  );

  // ── The command line, from the backend ──────────────────────────────────────
  //
  // Asked for rather than assembled here, because the backend's `argv` is the one place a cargo
  // command line is built and a preview that disagreed with what runs would be worse than none.
  //
  // The effect re-runs whenever a field `cargoInvocationOf` reads changes, which is exactly the set
  // that can change the answer. A sequence number drops a response that a newer request has already
  // overtaken — without it a slow answer repaints a line that is no longer true.
  let preview = $state('');
  let previewSeq = 0;

  $effect(() => {
    const invocation = cargoInvocationOf(config);
    const seq = ++previewSeq;
    // Something readable immediately, so the line does not blink empty between keystrokes.
    if (!preview) preview = `cargo ${invocation.command}`;
    void cargoPreview(invocation)
      .then((line) => {
        if (seq === previewSeq) preview = line;
      })
      .catch(() => {
        if (seq === previewSeq) preview = `cargo ${invocation.command}`;
      });
  });
</script>

<FormField
  label="Crate"
  hint="What `-p` names. The whole workspace when you pick none."
>
  <Select
    value={config.module}
    options={crateOptions}
    fill
    searchable={(workspace?.crates.length ?? 0) > 12}
    onchange={(v) => patch({ module: v })}
  />
</FormField>

<FormField label="Command" hint={def?.doc ?? 'The cargo subcommand this configuration runs.'}>
  <Select
    value={config.cargoCommand}
    options={commandOptions}
    fill
    onchange={(v) => patch({ cargoCommand: v, cargoTargetKind: '', cargoTarget: '' })}
  />
</FormField>

{#if missingComponent}
  <div class="cargo-notice">
    <Alert variant="warning" compact>
      This toolchain has no <code>{missingComponent}</code> component, so the command will fail with
      an unknown-subcommand error. Install it with
      <code>rustup component add {missingComponent}</code>, then reload the Cargo panel.
    </Alert>
  </div>
{/if}

{#if def?.targeted !== false}
  <FormField label="Target" hint="Which of the crate's targets to build. Cargo's default otherwise.">
    <div class="cargo-target">
      <Select
        value={config.cargoTargetKind}
        options={targetKindOptions}
        fill
        onchange={(v) => patch({ cargoTargetKind: v, cargoTarget: '' })}
      />
      {#if targetOptions.length > 0}
        <Select
          value={config.cargoTarget}
          options={[{ value: '', label: 'Choose…' }, ...targetOptions]}
          fill
          searchable={targetOptions.length > 12}
          onchange={(v) => patch({ cargoTarget: v })}
        />
      {/if}
    </div>
  </FormField>
  {#if targetIncomplete}
    <p class="cargo-warn">
      <TriangleAlert size={11} />
      Pick a {config.cargoTargetKind}, or the selector is dropped and the command builds whatever it
      defaults to.
    </p>
  {/if}
{/if}

{#if def?.featured !== false}
  <FormField label="Features" hint="Passed as `--features`. The crate's own defaults stay on unless you turn them off.">
    {#snippet actions()}
      <Dropdown items={featureItems} position="fixed" direction="down" width="260px">
        {#snippet trigger({ toggle, open })}
          <button
            class="pick-btn"
            class:open
            type="button"
            onclick={toggle}
            disabled={!featureItems.length}
            use:tooltip={featureItems.length
              ? 'Features this crate declares — click to add or remove'
              : crate
                ? 'This crate declares no features'
                : 'Pick a crate to see its features'}
            aria-haspopup="menu"
            aria-expanded={open}
          >
            Declared{featureItems.length ? ` (${featureItems.length})` : ''}
            <ChevronDown size={12} />
          </button>
        {/snippet}
      </Dropdown>
    {/snippet}
    <!-- Free text with the picker as a shortcut, for the same reason the Spring profiles field is:
         a feature can belong to a dependency (`serde/derive`) or not exist yet. -->
    <Input
      value={config.cargoFeatures}
      placeholder="std,derive"
      oninput={(v) => patch({ cargoFeatures: v })}
    />
  </FormField>

  <FormField label="Feature set">
    <label class="rc-check">
      <Toggle
        checked={config.cargoAllFeatures}
        onchange={(v) => patch({ cargoAllFeatures: v })}
      />
      <span>Every feature (<code>--all-features</code>)</span>
    </label>
    <label class="rc-check">
      <Toggle
        checked={config.cargoNoDefaultFeatures}
        onchange={(v) => patch({ cargoNoDefaultFeatures: v })}
      />
      <span>Drop the defaults (<code>--no-default-features</code>)</span>
    </label>
  </FormField>
{/if}

{#if def?.profiled !== false}
  <FormField
    label="Profile"
    hint="`--release` is the short spelling of the release profile. A named profile wins over it."
  >
    <div class="cargo-target">
      <label class="rc-check">
        <Toggle checked={config.cargoRelease} onchange={(v) => patch({ cargoRelease: v })} />
        <span>Release</span>
      </label>
      <Input
        value={config.cargoProfile}
        placeholder="a named profile, e.g. release-lto"
        oninput={(v) => patch({ cargoProfile: v })}
      />
    </div>
  </FormField>
{/if}

{#if def?.scoped !== false && !config.module.trim()}
  <FormField label="Scope" hint="A virtual workspace root builds nothing on its own, so this is usually on.">
    <label class="rc-check">
      <Toggle
        checked={config.cargoWorkspace}
        onchange={(v) => patch({ cargoWorkspace: v })}
      />
      <span>Every crate in the workspace (<code>--workspace</code>)</span>
    </label>
  </FormField>
{/if}

<FormField label="Cargo arguments" hint="Extra flags for cargo itself, before the `--`.">
  <Input
    value={config.cargoArgs}
    placeholder="--locked --offline"
    oninput={(v) => patch({ cargoArgs: v })}
  />
</FormField>

<FormField
  label="Program arguments"
  hint={def?.passes_args === false
    ? `cargo ${config.cargoCommand} passes nothing on, so these are ignored.`
    : 'After the `--`. Reaches the program, or the test harness (`--nocapture`).'}
>
  <Input
    value={config.programArgs}
    placeholder="--nocapture"
    oninput={(v) => patch({ programArgs: v })}
  />
</FormField>

<FormField label="Working directory" hint="Empty = the workspace root, which is where `-p` is resolved.">
  <Input
    value={config.workingDir}
    placeholder={workspace?.root ?? '/path/to/workspace'}
    oninput={(v) => patch({ workingDir: v })}
  />
</FormField>

<BennuRunEnvField env={config.env} onchange={(next: EnvVar[]) => patch({ env: next })} />

{#if !workspace}
  <p class="cargo-warn">
    <Terminal size={11} />
    The workspace has not been read yet, so the pickers are empty. Open the Cargo tool window once.
  </p>
{/if}

<!-- The real command line, from the same function that builds it for the run. -->
<p class="cmd-preview"><span class="cmd">{preview}</span></p>

<style>
  .cargo-notice { padding: 2px 0 4px; }
  /* Two controls that answer one question — a kind and then its name, a toggle and then an
     override. Side by side because reading them apart is what makes them look unrelated. */
  .cargo-target { display: flex; align-items: center; gap: 8px; }
  .cargo-target > :global(*) { min-width: 0; flex: 1; }
  .cargo-warn {
    display: flex; align-items: center; gap: 5px;
    margin: 0; font-size: var(--font-size-2xs); color: var(--warning);
  }
  .cargo-warn :global(svg) { flex-shrink: 0; }
  .rc-check {
    display: flex; align-items: center; gap: 8px;
    font-size: var(--font-size-sm); color: var(--text-secondary);
    cursor: pointer; padding: 2px 0;
  }
  code {
    font-family: var(--font-code); font-size: var(--font-size-2xs);
    color: var(--text-primary);
  }
  .pick-btn {
    display: inline-flex; align-items: center; gap: 4px;
    background: transparent; border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); padding: 2px 6px;
    font-size: var(--font-size-2xs); color: var(--text-secondary); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .pick-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .pick-btn.open { background: var(--bg-hover); color: var(--text-primary); }
  .pick-btn:disabled { opacity: 0.4; cursor: default; }
  .cmd-preview { margin: 2px 0 0; }
  .cmd {
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-muted);
    background: var(--bg-elevated); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); padding: 6px 9px; display: block;
    overflow-x: auto; white-space: nowrap;
  }
</style>
