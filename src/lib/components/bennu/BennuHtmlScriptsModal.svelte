<script lang="ts">
  /**
   * "Let this page's scripts run?" — the one consent the HTML preview asks for.
   *
   * ## Why this is not a ConfirmModal
   *
   * Because the honest answer has three shapes, not two. "Yes" to a page you are about to close
   * and "yes" to the coverage report you open every morning are different decisions, and
   * collapsing them means either asking the same question forever or remembering an answer that
   * was meant for one afternoon. So: **once**, **always**, or no.
   *
   * ## What the reader is agreeing to
   *
   * Not "trust this file" — the sandbox does not become weaker. The frame has no origin of its
   * own either way, so nothing in it can reach Arbor, its storage or the file tree. What changes
   * is that the page starts *doing* things: running its own code, and reaching the network.
   * That is the sentence the dialog leads with, because it is the only one that decides.
   */
  import { ShieldAlert } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';

  let {
    /** Absolute path of the page — its name is what the reader recognises. */
    path,
    /** Allow for this session only, or allow and remember (per file, across launches). */
    onAllow,
    onCancel,
  }: {
    path: string;
    onAllow: (remember: boolean) => void;
    onCancel: () => void;
  } = $props();

  const name = $derived(path.split(/[\\/]/).pop() ?? path);

  function onKey(e: KeyboardEvent) {
    // Enter takes the cautious one of the two yeses. The remembered answer is the one that
    // outlives the moment, and it should be pressed on purpose rather than fallen into.
    if (e.key === 'Enter') { e.preventDefault(); onAllow(false); }
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal onClose={onCancel} width="520px" height="auto" ariaLabel="Allow scripts">
  {#snippet header()}
    <ModalHeader onClose={onCancel}>
      <ShieldAlert size={14} />
      <span class="modal-title">Let this page run its scripts?</span>
    </ModalHeader>
  {/snippet}

  <div class="hs">
    <p class="hs-file">{name}</p>
    <p>
      The preview already renders. Allowing scripts is what lets the page <strong>act</strong>:
      run its own code, and reach the network.
    </p>
    <p class="hs-muted">
      The sandbox does not change. The frame has no origin of its own either way, so nothing in
      it can read Arbor, its storage, or your files — allow this for a page you know, and the
      worst it can do is to itself.
    </p>
  </div>

  {#snippet footer()}
    <ModalFooter>
      <Button variant="ghost" onclick={onCancel}>Cancel</Button>
      <Button variant="secondary" onclick={() => onAllow(false)}>Just this once</Button>
      <Button variant="primary" onclick={() => onAllow(true)}>Always for this file</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .hs { padding: 14px 18px 4px; display: flex; flex-direction: column; gap: 10px; }
  .hs p { margin: 0; font-size: var(--font-size-sm); color: var(--text-secondary); line-height: 1.55; }
  .hs-file { font-family: var(--font-code); color: var(--text-primary); }
  .hs-muted { color: var(--text-muted); font-size: var(--font-size-xs); }
</style>
