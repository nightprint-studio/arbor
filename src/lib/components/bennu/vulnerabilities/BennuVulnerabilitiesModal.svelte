<script lang="ts">
  /**
   * BennuVulnerabilitiesModal — every known vulnerability on the project's classpath, by severity.
   *
   * The editor underlines one dependency in one pom; this answers the question that actually
   * brings you here — "do I have a CVE, and how bad" — across declared dependencies (whatever
   * decides their version: a BOM, a parent, a property) and everything they drag in.
   *
   * The window itself is Corvus's: the shared `SecurityFindingsBrowser` (severity tabs, virtualized
   * rows, per-finding detail). This file fetches from OSV through the backend, maps advisories to
   * findings, and owns the one state Corvus does not have: the check being switched off.
   */
  import { onMount } from 'svelte';
  import { ShieldAlert, RefreshCw } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import SecurityFindingsBrowser from '$lib/components/shared/internal/security/SecurityFindingsBrowser.svelte';
  import { projectAdvisories, type ProjectAdvisories } from '$lib/ipc/bennu/maven';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuConfigStore } from '$lib/stores/bennu/config.svelte';
  import { advisoriesToFindings } from './advisory-findings';

  let { onClose }: { onClose: () => void } = $props();

  let data = $state<ProjectAdvisories | null>(null);
  let loading = $state(true);
  let error = $state('');

  async function load() {
    const root = projectStore.project?.root;
    if (!root) { loading = false; return; }
    loading = true;
    error = '';
    try {
      data = await projectAdvisories(root);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => { void load(); });

  const findings = $derived(advisoriesToFindings(data?.items ?? []));
  const off = $derived(!!data && !data.enabled);

  async function enableCheck() {
    const m = bennuConfigStore.cfg?.maven ?? { central: true, metadata_ttl_hours: 24, advisories: true };
    await bennuConfigStore.patch({ maven: { ...m, advisories: true } });
    await load();
  }
</script>

<Modal {onClose} width="min(94vw, 980px)" height="78vh" padBody={false} ariaLabel="Vulnerabilities">
  {#snippet header()}
    <ModalHeader {onClose}>
      <ShieldAlert size={14} />
      <span class="modal-title">Vulnerabilities</span>
      {#if data?.enabled}<span class="hdr-count">{findings.length}</span>{/if}
      {#snippet actions()}
        <Button variant="ghost" size="sm" onclick={() => void load()} disabled={loading} title="Check again">
          {#snippet iconStart()}<RefreshCw size={12} />{/snippet}
          Refresh
        </Button>
      {/snippet}
    </ModalHeader>
  {/snippet}

  {#if off}
    <div class="vl-off">
      <Alert variant="info" compact>
        The vulnerability check is off. Turning it on sends each dependency's
        <code>group:artifact</code> and version to api.osv.dev — nothing else about the project.
      </Alert>
      <div class="vl-enable"><Button variant="primary" size="sm" onclick={() => void enableCheck()}>Turn on</Button></div>
    </div>
  {:else}
    <SecurityFindingsBrowser
      {findings}
      {loading}
      {error}
      hideLifecycle
      noun="vulnerability"
      plural="vulnerabilities"
      emptyMessage={data ? `No known vulnerabilities in ${data.checked} checked artifacts.` : 'Nothing to show.'}
    >
      {#snippet note()}
        {#if data?.truncated}
          <span class="vl-trunc">A large classpath: only the first {data.checked} artifacts were checked.</span>
        {/if}
      {/snippet}
    </SecurityFindingsBrowser>
  {/if}
</Modal>

<style>
  .vl-off { padding: 14px; }
  .vl-enable { display: flex; justify-content: flex-end; padding-top: 8px; }
  .vl-trunc { color: var(--warning); }
  code { font-family: var(--font-code); font-size: var(--font-size-2xs); }
</style>
