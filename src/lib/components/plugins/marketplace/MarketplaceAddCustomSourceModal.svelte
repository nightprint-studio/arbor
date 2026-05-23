<!--
  MarketplaceAddCustomSourceModal — secondary modal for adding a third-party
  GitHub source to the marketplace catalogue.

  Owns the form state (repo / ref / subpath), the "resolving…" busy flag and
  the IPC call. Bubbles the resolved plugins up via `onResolved` so the host
  marketplace can merge them into its in-memory catalogue and pick the right
  success-toast copy (the modal can't know whether the host wants to render
  one or many resolved entries).

  Errors during resolution are surfaced inline as a toast since the modal
  stays open; the validation toast ("Repository URL is required") sits in
  the same flow.
-->
<script lang="ts">
  import { Plus } from 'lucide-svelte';
  import Modal       from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Input       from '$lib/components/shared/ui/Input.svelte';
  import Button      from '$lib/components/shared/ui/Button.svelte';
  import Alert       from '$lib/components/shared/ui/Alert.svelte';
  import Spinner     from '$lib/components/shared/ui/Spinner.svelte';
  import { addCustomSource as ipcAddCustomSource } from '$lib/ipc/marketplace';
  import { uiStore } from '$lib/stores/ui.svelte';
  import type { MarketplacePlugin } from '$lib/types/marketplace';

  interface Props {
    /** Fires with the resolved plugins after a successful resolve. The host
     *  is expected to merge them, show whatever success toast suits the
     *  copy, and close the modal. */
    onResolved: (plugins: MarketplacePlugin[]) => void;
    /** Fires on Cancel / backdrop dismiss / ESC. The host owns the open flag. */
    onClose:    () => void;
  }

  let { onResolved, onClose }: Props = $props();

  let repo      = $state('');
  let ref       = $state('');
  let subpath   = $state('');
  /** True while the backend is hitting GitHub to resolve the URL. */
  let resolving = $state(false);

  async function submit() {
    if (resolving) return;
    if (!repo.trim()) {
      uiStore.showToast('Repository URL is required.', 'error');
      return;
    }
    resolving = true;
    try {
      const resolved = await ipcAddCustomSource({
        repo:    repo.trim(),
        ref:     ref.trim()     || undefined,
        subpath: subpath.trim() || undefined,
      });
      onResolved(resolved);
    } catch (err) {
      uiStore.showToast(`Could not add custom source: ${err}`, 'error');
    } finally {
      resolving = false;
    }
  }
</script>

<Modal {onClose} width="560px" height="auto" ariaLabel="Add custom source">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Plus size={14} />
      <span class="modal-title">Add custom source</span>
    </ModalHeader>
  {/snippet}

  <!-- Wrap the form in <form> so Enter on any input submits via the primary
       button (Button defaults to type="button" — we mark this one as
       type="submit" explicitly). -->
  <form class="form" onsubmit={(e) => { e.preventDefault(); submit(); }}>
    <p class="form-hint">
      Point Arbor at any GitHub repo. The resolver looks for a <code>plugin.toml</code> at the root
      first, then for an <code>arbor-registry.toml</code> manifest (multi-plugin repos), and finally
      for a plugin at the subpath you specify below.
    </p>

    <label class="form-row">
      <span>Repository URL <span class="required">*</span></span>
      <Input
        bind:value={repo}
        placeholder="https://github.com/owner/repo"
        ariaLabel="Repository URL"
      />
    </label>

    <div class="form-grid">
      <label class="form-row">
        <span>Ref <small>(tag, branch or SHA — optional)</small></span>
        <Input
          bind:value={ref}
          placeholder="defaults to main"
          ariaLabel="Git ref"
        />
      </label>
      <label class="form-row">
        <span>Subpath <small>(optional, for monorepos)</small></span>
        <Input
          bind:value={subpath}
          placeholder="plugins/my-plugin"
          ariaLabel="Subpath"
        />
      </label>
    </div>

    <Alert variant="warning" compact>
      Custom sources are unverified — review the plugin's source on GitHub before enabling.
      Plugins are disabled by default after install.
    </Alert>
  </form>

  {#snippet footer()}
    <ModalFooter>
      <Button variant="ghost" disabled={resolving} onclick={onClose}>
        Cancel
      </Button>
      <Button
        variant="primary"
        type="submit"
        disabled={resolving || !repo.trim()}
        onclick={submit}
      >
        {#snippet iconStart()}
          {#if resolving}<Spinner size="xs" ariaLabel="Resolving source" />{:else}<Plus size={14} />{/if}
        {/snippet}
        {resolving ? 'Resolving…' : 'Add source'}
      </Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .form {
    padding: 16px 20px 8px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    /* Reset native <form> defaults. */
    margin: 0;
    border: none;
  }
  .form-hint {
    margin: 0;
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    line-height: 1.45;
  }
  .form-hint code {
    font-family: var(--font-mono);
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    font-size: 11px;
  }
  .form-row { display: flex; flex-direction: column; gap: 4px; }
  .form-row > span {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    color: var(--text-muted);
    font-weight: 600;
  }
  .form-row > span small {
    text-transform: none;
    letter-spacing: 0;
    color: var(--text-disabled);
    font-weight: 400;
    margin-left: 4px;
  }
  .form-row > span .required { color: var(--error); margin-left: 2px; }
  .form-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
</style>
