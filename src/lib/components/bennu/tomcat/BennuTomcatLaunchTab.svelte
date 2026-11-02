<script lang="ts">
  /**
   * The process around Tomcat: the JVM options, the environment it inherits, and the debugger.
   *
   * The classpath scope matters more here than for a plain run: an API the project marks
   * `provided` because an application server supplies it is, on Tomcat, supplied by nobody.
   *
   * The environment is the shared widget every other kind uses — a `.env` file and rows — because a
   * Tomcat's `JAVA_OPTS`-style variables are exactly the thing people already keep there.
   */
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import BennuRunEnvField from '../BennuRunEnvField.svelte';
  import BennuClasspathScopeField from '../BennuClasspathScopeField.svelte';
  import type { RunConfig } from '$lib/stores/bennu/run-config.svelte';
  import type { TomcatSettings } from '$lib/stores/bennu/tomcat-config';

  let {
    config,
    patch,
    patchT,
    root,
  }: {
    config: RunConfig;
    patch: (p: Partial<Omit<RunConfig, 'id'>>) => void;
    patchT: (p: Partial<TomcatSettings>) => void;
    root: string;
  } = $props();
</script>

<FormField label="JVM options" hint="For the Tomcat process: memory, system properties, encodings.">
  <Input
    value={config.tomcat.jvmArgs}
    placeholder="-Xms256m -Xmx1g -Dfile.encoding=UTF-8"
    oninput={(v) => patchT({ jvmArgs: v })}
  />
</FormField>

<BennuClasspathScopeField {config} {patch} />

<FormField label="Environment" hint="Variables the Tomcat process starts with.">
  <BennuRunEnvField {config} {patch} {root} />
</FormField>

<FormField label="Debugger" hint="Used by the Debug button. Off by default: a launch that begins frozen is the exception.">
  <label class="check">
    <Toggle checked={config.debugSuspend} onchange={(v) => patch({ debugSuspend: v })} />
    <span>Wait for the debugger before starting (<code>suspend=y</code>)</span>
  </label>
</FormField>

<style>
  .check { display: flex; align-items: center; gap: 8px; color: var(--text-secondary); }
  code { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); }
</style>
