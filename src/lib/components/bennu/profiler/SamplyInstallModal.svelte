<script lang="ts">
  /**
   * Consent and progress for downloading the official samply release.
   *
   * Shows exactly what will be fetched — version, file, size, source — and how it will be checked.
   * A release with no checksum to verify against is not installed on a plain "Install": the user
   * has to tick an explicit box first. Once started, the same dialog follows the download.
   */
  import { Download } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Checkbox from '$lib/components/shared/ui/Checkbox.svelte';
  import ProgressBar from '$lib/components/shared/ui/ProgressBar.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import UrlBlock from '$lib/components/shared/ui/UrlBlock.svelte';
  import { bennuSamplyStore as samply } from '$lib/stores/bennu/samply.svelte';
  import { fmtBytes } from './profiler-format';

  let { onClose }: { onClose: () => void } = $props();

  let acceptUnverified = $state(false);

  const latest = $derived(samply.latest);
  const unverified = $derived(latest?.checksum_source === 'none');
  const canInstall = $derived(
    !!latest && !latest.blocker && !samply.installing && (!unverified || acceptUnverified),
  );
  const phaseLabel = $derived(
    ({ lookup: 'Looking up the release…', download: 'Downloading…', verify: 'Verifying the checksum…', extract: 'Unpacking…' } as const)[samply.phase],
  );
  const pct = $derived(samply.total ? (samply.done / samply.total) * 100 : undefined);

  function start() {
    if (canInstall) void samply.install(unverified && acceptUnverified);
  }

  function close() {
    if (samply.installing) return;
    samply.dismissInstall();
    onClose();
  }

  // Close by itself once the install has landed (the store clears `latest` on success).
  let sawLatest = false;
  $effect(() => {
    if (latest) sawLatest = true;
    else if (sawLatest && !samply.checking && !samply.latestError && !samply.installing) onClose();
  });

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      start();
    }
  }
</script>

<Modal onClose={close} width="560px" height="auto" ariaLabel="Download samply">
  {#snippet header()}
    <ModalHeader onClose={close}>
      <Download size={14} />
      <span class="title">Download samply</span>
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="body" onkeydown={onKeydown}>
    {#if samply.checking}
      <div class="state"><Spinner size={13} /> Looking up the latest release…</div>
    {:else if samply.latestError}
      <Alert variant="error" compact>{samply.latestError}</Alert>
    {:else if latest}
      <dl class="facts">
        <dt>Version</dt>
        <dd>samply {latest.version}{#if latest.installed_version} <span class="muted">(installed: {latest.installed_version})</span>{/if}</dd>
        <dt>File</dt>
        <dd><code>{latest.asset_name}</code> · {fmtBytes(latest.size)}</dd>
      </dl>
      <UrlBlock label="Source" value={latest.url} copyable />
      <p class="note">
        Installed under Arbor's data folder; nothing outside it is touched. The official project is github.com/mstange/samply.
      </p>

      {#if latest.blocker}
        <Alert variant="error" compact>{latest.blocker}</Alert>
      {:else if latest.checksum_source === 'none'}
        <Alert variant="warning" compact title="No checksum could be obtained">
          This release publishes nothing to verify the download against, so its integrity cannot be checked.
        </Alert>
        <Checkbox bind:checked={acceptUnverified} label="Install it anyway, without verification" />
      {:else}
        <Alert variant="success" compact title="Will be verified">
          The download is checked against the SHA-256 {latest.checksum_source === 'api' ? 'reported by GitHub for this file' : 'published with the release'}:
          <code class="hash">{latest.sha256}</code>
        </Alert>
      {/if}

      {#if samply.installing}
        <div class="progress">
          <ProgressBar {pct} indeterminate={pct === undefined} ariaLabel="Download progress" />
          <span class="muted">{phaseLabel}{#if samply.phase === 'download' && samply.done} {fmtBytes(samply.done)}{#if samply.total} / {fmtBytes(samply.total)}{/if}{/if}</span>
        </div>
      {/if}
      {#if samply.installError}
        <Alert variant="error" compact>{samply.installError}</Alert>
      {/if}
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="end">
      <Button variant="secondary" size="sm" onclick={close} disabled={samply.installing}>Cancel</Button>
      <Button
        variant="primary"
        size="sm"
        onclick={start}
        disabled={!canInstall}
        loading={samply.installing}
        tooltip={{ content: 'Download and install', shortcut: 'Ctrl+Enter' }}
      >
        {latest?.installed_version ? 'Update' : 'Install'}
      </Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .body { display: flex; flex-direction: column; gap: 10px; }
  .state { display: flex; align-items: center; gap: 8px; color: var(--text-muted); padding: 12px 0; }
  .facts { display: grid; grid-template-columns: max-content 1fr; gap: 4px 12px; margin: 0; font-size: var(--font-size-sm); }
  .facts dt { color: var(--text-muted); }
  .facts dd { margin: 0; color: var(--text-primary); }
  .muted { color: var(--text-muted); font-size: var(--font-size-xs); }
  .note { margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); }
  .hash { display: block; margin-top: 4px; font-family: var(--font-code); font-size: var(--font-size-xs); word-break: break-all; }
  .progress { display: flex; flex-direction: column; gap: 4px; }
</style>
