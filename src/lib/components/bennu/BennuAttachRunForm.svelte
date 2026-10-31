<script lang="ts">
  /**
   * The `attach` arm of the run-configuration editor.
   *
   * An Attach configuration launches nothing. It connects the debugger to a JVM somebody already
   * started with a JDWP agent listening — the "Remote JVM Debug" of other IDEs — so below the name
   * there is exactly one thing to decide: where it listens. No module, class, classpath or
   * environment: those are the launcher's, and there is no launcher here.
   *
   * ## Loopback only, and said so
   *
   * The host is not a field. The backend connects to `127.0.0.1` and nowhere else, and a form that
   * offered a host it then ignored would be worse than one that says what it does. A JVM on another
   * machine can be reached through an SSH tunnel to a local port.
   *
   * ## The port is checked, not trusted
   *
   * The stepper clamps to 1–65535. The error line is for a value that did not come from it — a
   * hand-edited file — which reads as "no port" and is refused at attach time with the same words.
   */
  import { Plug } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import { attachPortOf, type RunConfig } from '$lib/stores/bennu/run-config.svelte';

  let { config, patch }: { config: RunConfig; patch: (p: Partial<RunConfig>) => void } = $props();

  const port = $derived(attachPortOf(config));
</script>

<Alert
  variant="info"
  compact
  text="Attaches the debugger to a JVM that is already running; nothing is launched, and detaching leaves the program running. Only this machine (127.0.0.1) is supported."
/>

<FormField
  label="Port"
  hint="The JDWP port of the JVM, 1–65535."
  error={port === null ? 'Not a port: use a number from 1 to 65535.' : null}
>
  <NumberStepper
    value={port}
    min={1}
    max={65535}
    step={1}
    narrow={false}
    ariaLabel="JDWP port"
    placeholder="5005"
    onchange={(v) => patch({ attachPort: String(Math.round(v)) })}
  >
    {#snippet iconStart()}<Plug size={13} />{/snippet}
  </NumberStepper>
</FormField>

<FormField label="Start the JVM with" hint="Add this to the command line of the program to debug (suspend=y waits for the debugger before main).">
  <code class="at-cmd">-agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:{port ?? 5005}</code>
</FormField>

<style>
  .at-cmd {
    display: block;
    padding: 6px 8px;
    border-radius: var(--radius-sm);
    background: var(--bg-overlay);
    color: var(--text-secondary);
    font: var(--font-size-xs) var(--font-code);
    user-select: text;
    word-break: break-all;
  }
</style>
