<!--
  PluginOverlays — every surface a plugin uses to talk to the user, in the one stacking
  order that works, mounted once per shell.

  `arbor.ui.form`, `arbor.ui.pick_file`, `arbor.ui.container.open` and
  `arbor.ui.copy_to_clipboard` all work the same way: the plugin calls, the backend emits an
  event, and a component in the window has to be listening. If nothing is, the plugin runs
  perfectly and **absolutely nothing happens** — no error, no log, because from the backend's
  side the message was delivered.

  That is not hypothetical. Bennu grew a plugin host, registered a command, showed it in the
  palette, fired the action — and the form went nowhere, because this cluster lived inline in
  Corvus's shell. A product that hosts plugins mounts this; that is the whole contract.

  ## The ordering is load-bearing

  These are all `<Modal>`-class overlays on `--z-modal-bg`, so document order alone decides
  what paints on top, and each pair here has a real case behind it:

  · the **form** comes after anything a plugin action can be fired *from* (the Plugin Manager,
    a contributed row action) — otherwise the form opens behind the modal that triggered it
    and the user sees a click that did nothing;
  · the **picker** comes after the form, because a plugin commonly opens a picker from inside
    its own form (source-export's "Import profile"), and on WebView2 the picker otherwise
    vanishes behind it;
  · the **container** comes last, because settings can be invoked from inside a form.

  ## What is deliberately NOT here

  Anything that resolves against one product's layout: `plugin:ui-open-panel` needs a sidebar
  to reveal the panel in, `plugin:ui-open-job-output` needs a bottom dock, and
  `plugin:ui-show-pipeline-run` needs Corvus's pipelines. Those stay in the shell that has the
  furniture. The line is: a plugin *talking to the user* is universal, a plugin *moving a
  product's panels* is not.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import Lazy from '$lib/components/shared/Lazy.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { pluginStore } from '$lib/stores/plugin.svelte';
  import { firePluginAction } from '$lib/ipc/plugin';
  import { containerStore } from '$lib/stores/corvus/container.svelte';
  import { setupTauriListeners } from '$lib/utils/tauri-listeners';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import type { PluginPickFile } from '$lib/stores/plugin.svelte';
  import type { PluginFormConfig } from '$lib/types/plugin';

  const pendingForm = $derived(pluginStore.pendingForm);
  const pickFile    = $derived(pluginStore.pickFile);

  /** Round-trip the picker's answer. An empty path IS the cancel signal. */
  function answerPicker(req: PluginPickFile, path: string) {
    const ctx = { path, ...(req.extra ?? {}) };
    // Reported by `firePluginAction` itself — swallowed here because a picker
    // that has already closed has nothing left to do about it.
    firePluginAction(req.plugin_name, req.action, JSON.stringify(ctx)).catch(() => {});
    pluginStore.clearPickFile();
  }

  onMount(() => setupTauriListeners([
    {
      event: 'plugin:form',
      handler: (e: { payload: PluginFormConfig }) => { pluginStore.setPendingForm(e.payload); },
    },
    {
      event: 'plugin:pick-file',
      handler: (e: { payload: PluginPickFile }) => { pluginStore.setPickFile(e.payload); },
    },
    {
      // `arbor.ui.copy_to_clipboard(text)` — the clipboard API lives in the webview, not in
      // Rust, so the write has to come back out here.
      event: 'plugin:ui-clipboard-write',
      handler: async (e: { payload: { plugin: string; text: string; toast?: string } }) => {
        const { text, toast } = e.payload;
        await copyToClipboard(text, { successToast: toast ?? 'Copied to clipboard', errorToast: true });
      },
    },
  ]));
</script>

<!-- The form a plugin opened. `#key` remounts when the config changes (one action opening a
     different form); after the first load the module is cached, so the swap is seamless. -->
{#if pendingForm}
  {#key pluginStore.formKey}
    <Lazy
      loader={() => import('./PluginFormModal.svelte')}
      form={pendingForm}
      onClose={() => pluginStore.clearPendingForm()}
    />
  {/key}
{/if}

<!-- `arbor.ui.pick_file`. Always FileExplorerModal, never the native dialog — see the
     working agreement: one file browser means one place to fix navigation. -->
{#if pickFile}
  {@const req = pickFile}
  <FileExplorerModal
    mode={req.mode ?? 'file'}
    title={req.title ?? 'Select a file'}
    extensions={req.extensions}
    initialPath={req.initial_path}
    onConfirm={(path) => answerPicker(req, path)}
    onCancel={() => answerPicker(req, '')}
  />
{/if}

<!-- `arbor.ui.container.open()` and its `arbor.ui.settings.open()` sugar. -->
{#if containerStore.openContainerId}
  {@const cid = containerStore.openContainerId}
  <Lazy
    loader={() => import('./ContributableModal.svelte')}
    containerId={cid}
    onClose={() => containerStore.close()}
  />
{/if}
