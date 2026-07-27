<script lang="ts">
  /**
   * Credentials — connect (or disconnect) a git host from any window.
   *
   * Cross-product by design: git credentials are shared by everything that
   * talks to a remote (Corvus and the File Explorer today), and the broker that
   * holds them is process-wide. Mounted in every window from `+page.svelte` and
   * raised through `credentialsStore`, so a token that expires while you are in
   * the File Explorer is fixable right there.
   *
   * Renders the same `GitProviderList` as Corvus's Settings ▸ Git, so the two
   * surfaces can't drift.
   */
  import { KeyRound } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import GitProviderList from '$lib/components/shared/internal/GitProviderList.svelte';
  import { credentialsStore } from '$lib/stores/credentials.svelte';

  const close = () => credentialsStore.close();
</script>

{#if credentialsStore.open}
  <Modal onClose={close} width="620px" height="min(720px, 88vh)" ariaLabel="Credentials">
    {#snippet header()}
      <ModalHeader onClose={close}>
        <KeyRound size={14} />
        <span class="modal-title">Credentials</span>
      </ModalHeader>
    {/snippet}

    {#snippet children()}
      <SectionHeader
        title="Git hosting"
        description="Connect a provider to sign in with your account. Tokens are stored in the OS keychain."
      />
      <GitProviderList />
    {/snippet}
  </Modal>
{/if}
