<script lang="ts">
  /**
   * The `tomcat` arm of the run-configuration editor.
   *
   * A Tomcat configuration shares a name, a module and an environment with every other kind and
   * brings a model of its own — a server, a deployment, the resources of a context — so, like the
   * cargo form, it is its own file and the modal only decides to show it. Four tabs, in the order a
   * person decides things: **Server** (which Tomcat, which ports), **Deployment** (what it serves),
   * **context.xml** (what JNDI offers it) and **Launch** (JVM options, environment, debugger).
   *
   * Everything the form knows about the outside — installations, what the project declares, ports in
   * use, stored passwords — lives in one shared object (`createTomcatForm`), so the badges on the
   * tabs and the errors under the fields are computed from the same facts.
   */
  import { Server, Rocket, FileCode2, Layers } from 'lucide-svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import type { RunConfig } from '$lib/stores/bennu/run-config.svelte';
  import type { TomcatSettings } from '$lib/stores/bennu/tomcat-config';
  import { createTomcatForm } from './tomcat/tomcat-form.svelte';
  import type { TomcatTab } from './tomcat/tomcat-issues';
  import BennuTomcatServerTab from './tomcat/BennuTomcatServerTab.svelte';
  import BennuTomcatDeployTab from './tomcat/BennuTomcatDeployTab.svelte';
  import BennuTomcatContextTab from './tomcat/BennuTomcatContextTab.svelte';
  import BennuTomcatLaunchTab from './tomcat/BennuTomcatLaunchTab.svelte';
  import BennuTomcatDownloadModal from './tomcat/BennuTomcatDownloadModal.svelte';

  let {
    config,
    patch,
    root,
    moduleOptions,
    isMultiModule,
  }: {
    config: RunConfig;
    patch: (p: Partial<Omit<RunConfig, 'id'>>) => void;
    root: string;
    moduleOptions: { value: string; label: string }[];
    isMultiModule: boolean;
  } = $props();

  const patchT = (p: Partial<TomcatSettings>) => patch({ tomcat: { ...config.tomcat, ...p } });
  const form = createTomcatForm(() => root, () => config);

  let tab = $state<TomcatTab>('server');
  let downloading = $state(false);

  // What the form knows about the outside, refreshed when the thing it depends on changes. Each
  // refresh guards against a late reply on its own (see `createTomcatForm`).
  $effect(() => {
    void config.tomcat.installDir;
    void config.tomcat.baseDir;
    void form.refreshEnv();
  });
  $effect(() => {
    void config.module;
    void form.refreshScan();
  });
  $effect(() => {
    void config.tomcat.home;
    void form.env;
    void form.refreshInspect();
  });
  $effect(() => {
    void config.tomcat.httpPort;
    void config.tomcat.shutdownPort;
    // Settled typing only — a port is asked about once it stops changing.
    const timer = setTimeout(() => void form.refreshPorts(), 300);
    return () => clearTimeout(timer);
  });
  $effect(() => {
    void config.id;
    void form.refreshSecrets();
  });

  // A new configuration with a Tomcat already installed starts on one: the newest whose servlet
  // package matches the project's, else the newest. Only when nothing is chosen — never over a choice.
  let autoChose = false;
  $effect(() => {
    const installs = form.env?.installs;
    if (autoChose || config.tomcat.home.trim() || !installs?.length || !form.scan) return;
    autoChose = true;
    const fitting = installs.find((i) => i.namespace === form.scan?.namespace) ?? installs[0];
    patchT({ home: fitting.home });
  });

  const issuesOn = (t: TomcatTab) => form.issues.filter((i) => i.tab === t);
  const badge = (t: TomcatTab): string | number | undefined => {
    const n = issuesOn(t).length;
    return n ? n : undefined;
  };

  const items = $derived<TabItem[]>([
    { id: 'server', label: 'Server', icon: Server, iconSize: 13, badge: badge('server') },
    { id: 'deploy', label: 'Deployment', icon: Layers, iconSize: 13, badge: badge('deploy') },
    { id: 'context', label: 'context.xml', icon: FileCode2, iconSize: 13, badge: badge('context') },
    { id: 'launch', label: 'Launch', icon: Rocket, iconSize: 13, badge: badge('launch') },
  ]);

  async function installed(home: string) {
    await form.refreshEnv();
    patchT({ home });
    downloading = false;
  }
</script>

<div class="tc">
  <Tabs {items} value={tab} variant="underline" size="sm" ariaLabel="Tomcat configuration" onSelect={(id) => (tab = id as TomcatTab)} />

  <div class="pane" class:fixed={tab === 'context'}>
    {#if tab === 'server'}
      <BennuTomcatServerTab {config} {patchT} {form} {root} onDownload={() => (downloading = true)} />
    {:else if tab === 'deploy'}
      <BennuTomcatDeployTab {config} {patch} {patchT} {form} {root} {moduleOptions} {isMultiModule} />
    {:else if tab === 'context'}
      <BennuTomcatContextTab {config} {patchT} {form} {root} />
    {:else}
      <BennuTomcatLaunchTab {config} {patch} {patchT} {root} />
    {/if}
  </div>
</div>

{#if downloading}
  <BennuTomcatDownloadModal
    {root}
    installDir={config.tomcat.installDir}
    lines={form.env?.lines ?? []}
    projectNamespace={form.scan?.namespace ?? null}
    onClose={() => (downloading = false)}
    onInstalled={installed}
  />
{/if}

<style>
  .tc { display: flex; flex-direction: column; gap: 14px; min-width: 0; flex: 1; min-height: 0; }
  /* The tabs stay in place and the pane under them scrolls; context.xml is the exception — it lays
     out columns that scroll on their own, so it gets the pane's height instead. */
  .pane { min-width: 0; flex: 1; min-height: 0; overflow-y: auto; }
  .pane.fixed { overflow: hidden; display: flex; flex-direction: column; }
</style>
