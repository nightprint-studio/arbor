<script lang="ts">
  /**
   * Bennu Tomcat link — per-project settings for JSP hot-swap.
   *
   * The user picks the Tomcat root (CATALINA_BASE, the folder holding `webapps/`); Bennu validates
   * it, lists the deployed web apps, and auto-selects the one this project maps to (by <finalName> /
   * artifactId / dir name, or the single deployed app). The link persists per-repo in
   * `<repo>/.arbor/config.toml` `[bennu.tomcat]` (CLAUDE.md rule #11) via `bennuTomcatStore`.
   *
   * Keyboard-first: <Modal> auto-focuses + owns Esc; Ctrl/Cmd+Enter saves; the folder pick uses the
   * shared FileExplorerModal (folder mode), no native dialog.
   */
  import { Server, FolderOpen, CircleCheck, TriangleAlert, Rocket } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuTomcatStore } from '$lib/stores/bennu/tomcat.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { detectTomcat, hotswapJsp, type TomcatDetection } from '$lib/ipc/bennu/tomcat';

  let { onClose }: { onClose: () => void } = $props();

  const project = $derived(projectStore.project);
  const root = $derived(project?.root ?? null);

  // Local editable draft, seeded from the store on open (Cancel = true no-op).
  let tomcatRoot = $state('');
  let webappName = $state(''); // '' = auto-detect
  let detection = $state<TomcatDetection | null>(null);
  let detecting = $state(false);
  let picking = $state(false);
  let deploying = $state(false);

  // Seed once from the store, then probe if already linked.
  let seeded = $state(false);
  $effect(() => {
    if (seeded || !root) return;
    seeded = true;
    void bennuTomcatStore.load(root).then((cfg) => {
      tomcatRoot = cfg.tomcat_root;
      webappName = cfg.webapp_name;
      if (cfg.tomcat_root.trim()) void probe(cfg.tomcat_root);
    });
  });

  async function probe(dir: string) {
    if (!root || !dir.trim()) { detection = null; return; }
    detecting = true;
    try {
      detection = await detectTomcat(root, dir);
      // Adopt the suggestion when the user hasn't pinned a webapp yet.
      if (!webappName && detection.suggested) webappName = detection.suggested;
    } catch {
      detection = null;
    } finally {
      detecting = false;
    }
  }

  // Debounce the as-you-type probe — each detect walks the webapp tree (JSP count), so we don't
  // want one per keystroke. The folder-pick + initial seed probe immediately.
  let probeTimer: ReturnType<typeof setTimeout> | null = null;
  function probeDebounced(dir: string) {
    if (probeTimer) clearTimeout(probeTimer);
    detection = null;
    probeTimer = setTimeout(() => void probe(dir), 400);
  }

  function onPicked(dir: string) {
    picking = false;
    tomcatRoot = dir;
    webappName = '';
    void probe(dir);
  }

  // Auto-detect + each deployed context. Kept in sync with the probe.
  const webappOptions = $derived([
    { value: '', label: detection?.suggested ? `Auto — detected (${detection.suggested})` : 'Auto — detect' },
    ...(detection?.webapps ?? []).map((w) => ({ value: w, label: w })),
  ]);

  const canSave = $derived(!!root && tomcatRoot.trim().length > 0 && (detection?.valid ?? false));

  async function save() {
    if (!root || !canSave) return;
    await bennuTomcatStore.save(root, { tomcat_root: tomcatRoot.trim(), webapp_name: webappName });
    onClose();
  }

  async function unlink() {
    if (!root) return;
    await bennuTomcatStore.save(root, { tomcat_root: '', webapp_name: '' });
    onClose();
  }

  async function saveAndDeployAll() {
    if (!root || !canSave || deploying) return;
    deploying = true;
    try {
      await bennuTomcatStore.save(root, { tomcat_root: tomcatRoot.trim(), webapp_name: webappName });
      await hotswapJsp(root); // BE fires the success/error toast
      onClose();
    } catch (e) {
      toastStore.show(`Hot-swap failed: ${e}`, 'error');
    } finally {
      deploying = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); void save(); }
  }
</script>

<Modal {onClose} width="640px" height="540px" ariaLabel="Tomcat hot-swap settings">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Server size={14} />
      <span class="modal-title">Tomcat hot-swap</span>
      {#if project}<span class="hdr-name">{project.name}</span>{/if}
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="body" onkeydown={handleKeydown}>
    {#if !project}
      <EmptyState message="Open a project to link a Tomcat." />
    {:else}
      <section class="cfg-section">
        <div class="sec-head"><Server size={13} /><h3>Tomcat server</h3></div>
        <FormField
          label="Tomcat root (CATALINA_BASE)"
          hint="The Tomcat install folder — the one containing webapps/, bin/, conf/."
        >
          <div class="row">
            <Input bind:value={tomcatRoot} placeholder="C:\\apache-tomcat-9" oninput={(v) => probeDebounced(v)} />
            <Button variant="secondary" size="sm" onclick={() => (picking = true)}>
              <FolderOpen size={13} /> Browse…
            </Button>
          </div>
        </FormField>

        {#if tomcatRoot.trim()}
          <div class="status">
            {#if detecting}
              <span class="muted">Checking…</span>
            {:else if detection?.valid}
              <span class="ok"><CircleCheck size={13} /> Valid Tomcat — {detection.webapps.length} web app(s) deployed</span>
            {:else}
              <span class="warn"><TriangleAlert size={13} /> No webapps/ here — not a Tomcat root</span>
            {/if}
          </div>
        {/if}
      </section>

      <section class="cfg-section">
        <div class="sec-head"><Rocket size={13} /><h3>Deployed web app</h3></div>
        <FormField
          label="Target context"
          hint="Which app under webapps/ this project deploys to. Auto picks it by name (finalName / artifactId / folder) or the only deployed app."
        >
          <Select bind:value={webappName} options={webappOptions} disabled={!detection?.valid} />
        </FormField>

        <div class="facts">
          <div class="fact">
            <span class="fk">Source JSP root</span>
            <span class="fv" title={detection?.source_webapp ?? ''}>
              {detection?.source_webapp || '—'}
            </span>
          </div>
          <div class="fact">
            <span class="fk">JSPs to deploy</span>
            <span class="fv">{detection?.jsp_count ?? 0}</span>
          </div>
        </div>
      </section>

      <p class="note">
        Hot-swap copies changed JSPs into the deployed webapp; Tomcat recompiles them on the next
        request — no redeploy or restart. Bind a key to <strong>Deploy current JSP</strong> (Ctrl+Shift+F10).
      </p>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <Button variant="ghost" size="sm" onclick={unlink} disabled={!bennuTomcatStore.isLinked(root ?? '')}>
        Unlink
      </Button>
      <div class="footer-actions">
        <Button variant="secondary" size="sm" onclick={onClose}>Cancel</Button>
        <Button variant="secondary" size="sm" onclick={saveAndDeployAll} disabled={!canSave || deploying}>
          {deploying ? 'Deploying…' : 'Save & deploy all'}
        </Button>
        <Button
          variant="primary"
          size="sm"
          onclick={save}
          disabled={!canSave}
          tooltip={{ content: 'Save', shortcut: 'Ctrl+Enter' }}
        >
          Save
        </Button>
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

{#if picking}
  <FileExplorerModal
    mode="folder"
    title="Select Tomcat root"
    initialPath={tomcatRoot || undefined}
    onConfirm={onPicked}
    onCancel={() => (picking = false)}
    onClose={() => (picking = false)}
  />
{/if}

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .hdr-name {
    font-size: 11px; color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .body { display: flex; flex-direction: column; gap: 18px; }
  .cfg-section { display: flex; flex-direction: column; gap: 10px; }
  .sec-head { display: flex; align-items: center; gap: 7px; color: var(--text-secondary); }
  .sec-head h3 { margin: 0; font-size: 12px; font-weight: 600; letter-spacing: 0.02em; color: var(--text-primary); }
  .row { display: flex; align-items: center; gap: 8px; }
  .row :global(> :first-child) { flex: 1; min-width: 0; }
  .status { font-size: 11.5px; }
  .status .ok { display: inline-flex; align-items: center; gap: 5px; color: var(--success); }
  .status .warn { display: inline-flex; align-items: center; gap: 5px; color: var(--warning); }
  .status .muted { color: var(--text-muted); }
  .facts {
    display: grid; grid-template-columns: 1fr 1fr; gap: 10px;
    border: 1px solid var(--border-subtle); border-radius: var(--radius-md); padding: 10px 12px;
  }
  .fact { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .fk { font-size: 10px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-muted); }
  .fv {
    font-size: 12px; color: var(--text-primary); font-family: var(--font-code);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .note { margin: 0; font-size: 11px; line-height: 1.5; color: var(--text-muted); }
  .footer-actions { display: flex; align-items: center; gap: 8px; }
</style>
