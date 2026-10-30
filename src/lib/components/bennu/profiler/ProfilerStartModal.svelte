<script lang="ts">
  /**
   * Profile… — what to record when the title bar's profiler button is not the answer: a Node or
   * native command, a JVM that is already running, or Java with its own settings.
   *
   * Java records the selected run configuration under Flight Recorder, or attaches to a JVM that is
   * already running; Node and native programs take a command, which runs in the Run console with the
   * profiler's environment or wrapper added. The modal closes once a recording starts: from then on
   * the Profiler panel shows the state and the Stop button.
   */
  import { Circle, Gauge, Plug } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import { bennuProfilerStore as store } from '$lib/stores/bennu/profiler.svelte';
  import { splitArgs } from '$lib/stores/bennu/run-config.svelte';
  import type { ProfileLang } from '$lib/ipc/bennu/profiler';
  import ProfilerSamplyTool from './ProfilerSamplyTool.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const lang = $derived(store.startLang);
  let command = $state('');
  let pid = $state('');

  const detect = $derived(store.detect);
  const idle = $derived(store.phase === 'idle');
  const javaConfig = $derived(store.activeJavaConfig());
  const javaOk = $derived(detect?.java.available ?? false);
  /** The execution sampler's interval. Shorter catches shorter methods and queries, and costs overhead. */
  const intervals = [
    { value: '', label: '10 ms (default)' },
    { value: '5', label: '5 ms' },
    { value: '2', label: '2 ms' },
    { value: '1', label: '1 ms — heaviest' },
  ];
  const jvmOptions = $derived([
    { value: '', label: store.jvms.length ? 'Choose a running JVM…' : 'No local JVM found' },
    ...store.jvms.map((j) => ({ value: String(j.pid), label: `${j.pid} — ${j.name}` })),
  ]);

  function recordConfig() {
    if (!javaConfig) return;
    void store.recordConfig(javaConfig);
    onClose();
  }

  function recordCommand() {
    if (lang === 'java' || !command.trim()) return;
    void store.recordCommand(lang, splitArgs(command), command.trim() || lang);
    onClose();
  }

  function attach() {
    const n = Number(pid);
    if (!(n > 0)) return;
    void store.attach(n, store.jvms.find((j) => j.pid === n)?.name ?? `pid ${n}`);
    onClose();
  }

  /** Ctrl+Enter does the tab's main action. */
  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Enter' || !(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    if (lang === 'java') {
      if (pid) attach();
      else recordConfig();
    } else {
      recordCommand();
    }
  }
</script>

<Modal {onClose} width="680px" height="500px" ariaLabel="Profile">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Gauge size={14} />
      <span class="modal-title">Profile</span>
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="body" onkeydown={onKey}>
    <Tabs
      size="sm"
      variant="pill"
      value={lang}
      ariaLabel="Language"
      items={[{ id: 'java', label: 'Java (JFR)' }, { id: 'node', label: 'Node' }, { id: 'rust', label: 'Rust (samply)' }]}
      onSelect={(id) => store.setStartLang(id as ProfileLang)}
    />

    {#if lang === 'java'}
      {#if detect && !detect.java.available}
        <Alert variant="warning" compact title="Flight Recorder is not available for this JDK">
          {detect.java.reason}
          {#if detect.java.remedy}<br />{detect.java.remedy}{/if}
        </Alert>
      {/if}
      <FormRow label="Run configuration" description="The application or Spring Boot configuration selected in the title bar.">
        <Button size="sm" variant="primary" disabled={!idle || !javaConfig || !javaOk} onclick={recordConfig}>
          {#snippet iconStart()}<Circle size={11} fill="currentColor" />{/snippet}
          {javaConfig ? `Record “${javaConfig.name}”` : 'Record'}
        </Button>
      </FormRow>
      <FormRow
        label="Sampling interval"
        description="How often the profiler looks at what each thread is running. A shorter interval also reports shorter waits on sockets and files, at the cost of more overhead. Needs JDK 17 or newer; older ones keep 10 ms."
      >
        <Select
          size="sm"
          value={store.samplingMs === null ? '' : String(store.samplingMs)}
          options={intervals}
          disabled={!idle}
          onchange={(v) => store.setSamplingMs(v ? Number(v) : null)}
        />
      </FormRow>
      {#if !javaConfig && javaOk}
        <Alert variant="info" compact>Select an application or Spring Boot run configuration in the title bar, or attach to a running JVM below.</Alert>
      {/if}
      <FormRow label="A running JVM" description="Records inside it with jcmd — it needs a JDK, and the target must run as you." stacked>
        <div class="attach">
          <Select size="sm" value={pid} options={jvmOptions} disabled={!idle || !javaOk || !detect?.can_attach} onchange={(v) => (pid = v)} />
          <Button size="sm" variant="outline" disabled={!idle || !pid} onclick={attach}>
            {#snippet iconStart()}<Plug size={11} />{/snippet}
            Attach
          </Button>
          <Button size="sm" variant="ghost" onclick={() => void store.listJvms()}>Refresh</Button>
        </div>
      </FormRow>
    {:else}
      <FormRow label="Command" description={lang === 'node' ? 'Runs with NODE_OPTIONS=--cpu-prof.' : 'Wrapped in samply record.'} stacked>
        <div class="cmd">
          <Input
            size="sm"
            value={command}
            oninput={(v) => (command = v)}
            autofocus
            placeholder={lang === 'node' ? 'node server.js   or   npm run dev' : 'target/release/my-app --flag'}
            ariaLabel="Command to profile"
          />
          <Button size="sm" variant="primary" disabled={!idle || !command.trim()} onclick={recordCommand}>
            {#snippet iconStart()}<Circle size={11} fill="currentColor" />{/snippet}
            Record
          </Button>
        </div>
      </FormRow>
      {#if lang === 'node' && detect && !detect.node}
        <Alert variant="warning" compact title="Node.js was not found">Install Node.js (v12 or newer) and put it on PATH to record JavaScript. A .cpuprofile made elsewhere can be imported.</Alert>
      {:else if lang === 'node'}
        <Alert variant="info" compact>
          V8 writes the profile when the process exits, so stop a dev server with its own shutdown (Ctrl+C in the console), not the Stop button.
        </Alert>
      {:else}
        <ProfilerSamplyTool />
      {/if}
    {/if}
    {#if store.error}
      <Alert variant="error" compact>{store.error}</Alert>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="end">
      <Button variant="secondary" size="sm" onclick={onClose}>Close</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .body { display: flex; flex-direction: column; gap: 12px; }
  .attach { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .cmd { display: flex; align-items: center; gap: 8px; width: 100%; }
</style>
