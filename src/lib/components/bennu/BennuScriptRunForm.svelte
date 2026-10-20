<script lang="ts">
  /**
   * The `script` arm of the run-configuration editor.
   *
   * Its own file for the same reason the cargo one is: below the name field a script
   * configuration shares nothing with a JVM launch — no module, no classpath, no VM arguments —
   * and `BennuRunConfigModal` is already the largest component in Bennu.
   *
   * ## The file is picked, not typed
   *
   * A project's scripts are scattered by nature (`scripts/`, `bin/`, beside the module they
   * build), and the one thing a typo here produces is a run that fails on "not a file" — so the
   * field carries a picker, and it opens where the last answer was rather than at the root.
   *
   * ## The shell is chosen from what the file can use
   *
   * The backend lists the shells that can interpret this extension and which of them this machine
   * has — `bash`, `zsh`, `sh` for a `.sh`; PowerShell 7 or Windows PowerShell for a `.ps1`; only
   * `cmd` for a `.bat`, which therefore shows no choice. `Auto` (the default) is what every script
   * did before there was a choice. An unavailable shell is shown, disabled, with the reason: a
   * configuration written on another machine may name one, and hiding it would make the field look
   * empty.
   *
   * ## What can run is answered at launch, not here
   *
   * A `.bat` is `cmd.exe` syntax and a `.sh` on Windows needs Git Bash — but a configuration
   * written on one machine is still the right configuration on another, so this form never
   * refuses a file. The launch is where a machine that cannot answers with what it looked for.
   */
  import {
    FolderOpen, Terminal, SquareTerminal, Command, Hash, Zap, AppWindow, Wand2, FileCode2, Cog,
  } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import BennuRunEnvField from './BennuRunEnvField.svelte';
  import { scriptShells, type ScriptShell } from '$lib/ipc/bennu';
  import type { RunConfig } from '$lib/stores/bennu/run-config.svelte';

  let {
    config,
    patch,
    /** The project root — where the picker starts when the configuration names no file yet. */
    root,
  }: {
    config: RunConfig;
    patch: (p: Partial<RunConfig>) => void;
    root: string;
  } = $props();

  let picking = $state(false);

  const ext = $derived((config.scriptFile.split('.').pop() ?? '').toLowerCase());
  const known = $derived(['sh', 'bash', 'bat', 'cmd', 'ps1'].includes(ext));

  /** What this machine has for the file's extension. Empty until the backend answers, and for an
   *  extension nothing here runs. */
  let shells = $state<ScriptShell[]>([]);
  $effect(() => {
    const file = config.scriptFile.trim();
    if (!file || !known) { shells = []; return; }
    let live = true;
    scriptShells(file)
      .then((list) => { if (live) shells = list; })
      .catch(() => { if (live) shells = []; });
    return () => { live = false; };
  });

  const SHELL_ICONS: Record<string, unknown> = {
    bash: SquareTerminal, zsh: Command, sh: Hash, pwsh: Zap, powershell: AppWindow, cmd: Terminal,
  };

  /** The one `Auto` resolves to — the first shell the machine has. */
  const autoPick = $derived(shells.find((s) => s.available) ?? null);

  /** A batch file has one interpreter, so there is nothing to choose. */
  const choosable = $derived(shells.length > 1);

  const shellOptions = $derived([
    {
      value: '',
      label: 'Auto',
      icon: Wand2,
      description: autoPick ? `From the extension — ${autoPick.label}` : 'From the extension',
    },
    ...shells.map((s) => ({
      value: s.id,
      label: s.label,
      icon: SHELL_ICONS[s.id] ?? Terminal,
      description: s.available ? s.detail : 'Not found on this machine',
      disabled: !s.available,
    })),
  ]);

  /** What `Auto` was refused for, when nothing at all can run the file. */
  const nothingRuns = $derived(shells.length > 0 && !autoPick);
</script>

<FormField label="Script" hint="The file to run. Its extension decides which shells can interpret it.">
  {#snippet actions()}
    <button class="pick-btn" type="button" onclick={() => (picking = true)}>
      <FolderOpen size={12} /> Browse…
    </button>
  {/snippet}
  <Input
    value={config.scriptFile}
    placeholder="{root}/scripts/deploy.sh"
    oninput={(v) => patch({ scriptFile: v })}
  >
    {#snippet iconStart()}<FileCode2 size={13} />{/snippet}
  </Input>
</FormField>

{#if config.scriptFile.trim() && !known}
  <Alert
    variant="warning"
    compact
    text="Bennu runs .sh, .bash, .bat, .cmd and .ps1. This one will be refused at launch."
  />
{/if}

{#if choosable}
  <FormField
    label="Run with"
    hint={config.scriptShell
      ? undefined
      : 'On Windows a .sh runs under Git Bash — the System32 bash is WSL and sees a different filesystem.'}
  >
    <RadioGroup
      value={config.scriptShell}
      appearance="card"
      options={shellOptions}
      onchange={(v) => patch({ scriptShell: v })}
    />
  </FormField>
  {#if nothingRuns}
    <Alert
      variant="warning"
      compact
      text="None of these shells was found on this machine. The launch will say what it looked for."
    />
  {/if}
{:else if shells.length === 1}
  <p class="sf-note"><Terminal size={12} /> Runs with {shells[0].label}{shells[0].available ? '' : ' — not available on this machine'}.</p>
{/if}

<FormField label="Arguments" hint="Passed to the script, after its name.">
  <Input
    value={config.programArgs}
    placeholder="--env staging"
    oninput={(v) => patch({ programArgs: v })}
  />
</FormField>

<FormSection
  label="Working directory"
  collapsible
  open={config.workingDir.trim().length > 0}
  summary={config.workingDir.trim() || "the script's folder"}
>
  {#snippet icon()}<Cog size={12} />{/snippet}
  <FormField hint="Empty = the script's own folder, which is what a script reading ./config expects.">
    <Input
      value={config.workingDir}
      placeholder={root}
      oninput={(v) => patch({ workingDir: v })}
    />
  </FormField>
</FormSection>

<BennuRunEnvField {config} {patch} {root} />

{#if picking}
  <FileExplorerModal
    mode="file"
    title="Pick a script"
    initialPath={config.scriptFile.trim()
      ? config.scriptFile.replace(/[\\/][^\\/]*$/, '')
      : root}
    extensions={['sh', 'bash', 'bat', 'cmd', 'ps1']}
    onConfirm={(file) => { patch({ scriptFile: file, scriptShell: '' }); picking = false; }}
    onCancel={() => (picking = false)}
    onClose={() => (picking = false)}
  />
{/if}

<style>
  .sf-note {
    display: flex; align-items: center; gap: 6px;
    margin: -4px 0 2px;
    font-size: var(--font-size-xs); color: var(--text-muted);
  }
  /* Matches the picker buttons in the JVM arm of the same form — one shape for "choose it for
     me" wherever it appears in this dialog. */
  .pick-btn {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 2px 8px; border-radius: var(--radius-sm);
    border: 1px solid var(--border-subtle); background: transparent;
    color: var(--text-secondary); font-size: var(--font-size-2xs); cursor: pointer;
  }
  .pick-btn:hover { color: var(--text-primary); border-color: var(--border); background: var(--bg-hover); }
</style>
