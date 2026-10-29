<script lang="ts">
  /**
   * What to record and the Record / Stop control.
   *
   * Java records the selected run configuration under Flight Recorder, or attaches to a JVM that is
   * already running; Node and native programs take a command, which runs in the Run console with
   * the profiler's environment or wrapper added.
   */
  import { Circle, Square, Plug } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import ProfilerSamplyTool from './ProfilerSamplyTool.svelte';
  import { bennuProfilerStore as store } from '$lib/stores/bennu/profiler.svelte';
  import { splitArgs } from '$lib/stores/bennu/run-config.svelte';
  import type { ProfileLang } from '$lib/ipc/bennu/profiler';

  let lang = $state<ProfileLang>('java');
  let command = $state('');
  let pid = $state('');

  const detect = $derived(store.detect);
  const idle = $derived(store.phase === 'idle');
  const javaConfig = $derived(store.activeJavaConfig());
  const javaOk = $derived(detect?.java.available ?? false);
  const jvmOptions = $derived([
    { value: '', label: store.jvms.length ? 'Attach to a running JVM…' : 'No local JVM found' },
    ...store.jvms.map((j) => ({ value: String(j.pid), label: `${j.pid} — ${j.name}` })),
  ]);

  function record() {
    if (lang === 'java') {
      if (javaConfig) void store.recordConfig(javaConfig);
    } else {
      void store.recordCommand(lang, splitArgs(command), command.trim() || lang);
    }
  }

  function attach() {
    const n = Number(pid);
    if (n > 0) void store.attach(n, store.jvms.find((j) => j.pid === n)?.name ?? `pid ${n}`);
  }
</script>

<div class="rec">
  <div class="rec-row">
    <Tabs
      size="sm"
      variant="pill"
      value={lang}
      ariaLabel="Language"
      items={[{ id: 'java', label: 'Java (JFR)' }, { id: 'node', label: 'Node' }, { id: 'rust', label: 'Rust (samply)' }]}
      onSelect={(id) => (lang = id as ProfileLang)}
    />

    {#if store.phase === 'recording'}
      <span class="live"><Circle size={9} fill="currentColor" /> Recording</span>
      <Button size="sm" variant="danger" onclick={() => void store.stop()}>
        {#snippet iconStart()}<Square size={11} />{/snippet}
        Stop and read
      </Button>
    {:else if store.phase === 'parsing'}
      <Spinner size={13} label={store.progress ? `Reading the recording… ${store.progress.toLocaleString()} events` : 'Reading the recording…'} />
    {:else if store.phase === 'preparing'}
      <Spinner size={13} label="Preparing…" />
    {:else if lang === 'java'}
      <Button size="sm" variant="primary" disabled={!idle || !javaConfig || !javaOk} onclick={record}
        title={javaConfig ? `Run “${javaConfig.name}” with Flight Recorder` : 'Select an application run configuration in the title bar'}>
        {#snippet iconStart()}<Circle size={11} fill="currentColor" />{/snippet}
        {javaConfig ? `Record “${javaConfig.name}”` : 'Record'}
      </Button>
      <Select size="sm" value={pid} options={jvmOptions} disabled={!idle || !javaOk || !detect?.can_attach}
        onchange={(v) => (pid = v)} />
      <Button size="sm" variant="outline" disabled={!idle || !pid} onclick={attach}>
        {#snippet iconStart()}<Plug size={11} />{/snippet}
        Attach
      </Button>
      <Button size="sm" variant="ghost" onclick={() => void store.listJvms()}>Refresh JVMs</Button>
    {:else}
      <div class="cmd">
        <Input
          size="sm"
          value={command}
          oninput={(v) => (command = v)}
          placeholder={lang === 'node' ? 'node server.js   or   npm run dev' : 'target/release/my-app --flag'}
          ariaLabel="Command to profile"
          onkeydown={(e) => { if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) record(); }}
        />
      </div>
      <Button size="sm" variant="primary" disabled={!idle || !command.trim()} onclick={record}>
        {#snippet iconStart()}<Circle size={11} fill="currentColor" />{/snippet}
        Record
      </Button>
    {/if}
  </div>

  {#if lang === 'java' && detect && !detect.java.available}
    <Alert variant="warning" compact title="Flight Recorder is not available for this JDK">
      {detect.java.reason}
      {#if detect.java.remedy}<br />{detect.java.remedy}{/if}
    </Alert>
  {:else if lang === 'java' && detect && detect.java.available && !javaConfig && idle}
    <Alert variant="info" compact>Select an application or Spring Boot run configuration in the title bar, or attach to a running JVM.</Alert>
  {:else if lang === 'node' && detect && !detect.node}
    <Alert variant="warning" compact title="Node.js was not found">Install Node.js (v12 or newer) and put it on PATH to record JavaScript. A .cpuprofile made elsewhere can be imported.</Alert>
  {:else if lang === 'node' && idle}
    <Alert variant="info" compact>
      The command runs with NODE_OPTIONS=--cpu-prof; V8 writes the profile when the process exits, so stop a dev server with its own shutdown
      (Ctrl+C in the console), not the Stop button.
    </Alert>
  {/if}
  {#if lang === 'rust'}
    <ProfilerSamplyTool />
  {/if}
  {#if store.error}
    <Alert variant="error" compact>{store.error}</Alert>
  {/if}
</div>

<style>
  .rec { display: flex; flex-direction: column; gap: 6px; padding: 6px 8px; }
  .rec-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .cmd { flex: 1; min-width: 220px; max-width: 520px; }
  .live { display: inline-flex; align-items: center; gap: 5px; color: var(--danger); font-size: var(--font-size-xs); }
</style>
