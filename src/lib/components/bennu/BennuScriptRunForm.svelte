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
   * ## What can run is answered at launch, not here
   *
   * A `.bat` is `cmd.exe` syntax and a `.sh` on Windows needs Git Bash — but a configuration
   * written on one machine is still the right configuration on another, so this form never
   * refuses a file. It says what will interpret it, which is the useful half, and the launch is
   * where a machine that cannot answers with what it looked for.
   */
  import { FolderOpen, Terminal } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import BennuRunEnvField from './BennuRunEnvField.svelte';
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

  /** What will interpret this file, in the words the launch would use. `null` for an extension
   *  nothing here runs — said as a warning rather than by refusing the value, because the file
   *  may simply not exist yet on this machine. */
  const interpreter = $derived.by(() => {
    const ext = (config.scriptFile.split('.').pop() ?? '').toLowerCase();
    if (ext === 'sh' || ext === 'bash') {
      return 'bash — on Windows, the one Git Bash provides (the System32 one is WSL and sees a different filesystem).';
    }
    if (ext === 'bat' || ext === 'cmd') return 'cmd.exe — Windows only.';
    if (ext === 'ps1') return 'PowerShell — powershell.exe on Windows, pwsh elsewhere.';
    return null;
  });
</script>

<FormField label="Script" hint="The file to run. Its extension decides what interprets it.">
  {#snippet actions()}
    <button class="pick-btn" type="button" onclick={() => (picking = true)}>
      <FolderOpen size={12} /> Browse…
    </button>
  {/snippet}
  <Input
    value={config.scriptFile}
    placeholder="{root}/scripts/deploy.sh"
    oninput={(v) => patch({ scriptFile: v })}
  />
</FormField>

{#if config.scriptFile.trim()}
  {#if interpreter}
    <p class="sf-note"><Terminal size={12} /> {interpreter}</p>
  {:else}
    <Alert
      variant="warning"
      compact
      text="Bennu runs .sh, .bash, .bat, .cmd and .ps1. This one will be refused at launch."
    />
  {/if}
{/if}

<FormField label="Arguments" hint="Passed to the script, after its name.">
  <Input
    value={config.programArgs}
    placeholder="--env staging"
    oninput={(v) => patch({ programArgs: v })}
  />
</FormField>

<FormField label="Working directory" hint="Empty = the script's own folder, which is what a script reading ./config expects.">
  <Input
    value={config.workingDir}
    placeholder={root}
    oninput={(v) => patch({ workingDir: v })}
  />
</FormField>

<BennuRunEnvField env={config.env} onchange={(next) => patch({ env: next })} />

{#if picking}
  <FileExplorerModal
    mode="file"
    title="Pick a script"
    initialPath={config.scriptFile.trim()
      ? config.scriptFile.replace(/[\\/][^\\/]*$/, '')
      : root}
    extensions={['sh', 'bash', 'bat', 'cmd', 'ps1']}
    onConfirm={(file) => { patch({ scriptFile: file }); picking = false; }}
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
