<!--
  The consent prompt: an AI client wants to run a tool that changes something.

  Lives in `shared/` and is mounted by `GlobalOverlays` in EVERY window, for the same
  reason the credentials dialog is: the endpoint is process-wide, and a tool call is
  parked on the answer with its timeout already running, so the prompt cannot depend on
  one particular window being open. It used to sit in the launcher — a window that
  closes the moment a product tab opens. The backend elects one window to ask in and
  emits only there, so mounting it everywhere does not mean prompting everywhere.

  Three things it must get right, because this is the moment the whole permission model
  reduces to:

  1. **Show the arguments.** The tool name is not what is being approved — "write a file"
     and "write *this* file" are different questions, and only the second one can be
     answered.
  2. **Deny on close.** Escape, the backdrop, the X: every exit is a no. A prompt whose
     dismissal means "yes" is not a prompt.
  3. **Keyboard-first, with the safe key as the default.** Enter denies. Approving is a
     deliberate act (Ctrl/Cmd+Enter, or the button), because the failure mode here is a
     user holding Enter through a dialog they did not read.
-->
<script lang="ts">
  import { AlertTriangle, FileWarning, ShieldQuestion } from 'lucide-svelte';

  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { mcpStore } from '$lib/stores/mcp.svelte';

  const request = $derived(mcpStore.pending);
  const queued  = $derived(mcpStore.queued);

  /** Reset per prompt: a grant meant for one tool must not carry to the next. */
  let remember = $state(false);
  $effect(() => {
    // Re-runs when the head of the queue changes.
    void request?.id;
    remember = false;
  });

  /** Badge tone. `error` rather than `danger` — that is what Badge's scale calls it. */
  const tone = $derived<'error' | 'warning' | 'info'>(
    request?.safety === 'destructive' ? 'error' : request?.safety === 'write' ? 'warning' : 'info',
  );

  /** The approve button escalates with the class: a destructive tool gets the red one. */
  const approveVariant = $derived<'danger' | 'primary'>(
    request?.safety === 'destructive' ? 'danger' : 'primary',
  );

  /** Capitalised so it can be rendered directly as a component. */
  const Icon = $derived(
    request?.safety === 'destructive' ? AlertTriangle : request?.safety === 'write' ? FileWarning : ShieldQuestion,
  );

  async function deny()  { await mcpStore.answer(false); }
  async function allow() { await mcpStore.answer(true, remember); }

  function onKeydown(e: KeyboardEvent) {
    if (!request) return;
    // Ctrl/Cmd+Enter approves; plain Enter and Escape both refuse.
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); void allow(); return; }
    if (e.key === 'Enter') { e.preventDefault(); void deny(); }
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if request}
  <Modal onClose={deny} width="600px" height="560px" ariaLabel="AI tool permission request">
    {#snippet header()}
      <ModalHeader title="Allow this action?" onClose={deny}>
        {#snippet actions()}
          <Badge tone={tone}>{request.safety}</Badge>
        {/snippet}
      </ModalHeader>
    {/snippet}

    <div class="body">
      <div class="lede">
        <Icon size={20} />
        <div>
          <p class="what">
            An AI client is asking to run <strong>{request.title}</strong>
            <span class="tool">({request.tool})</span> through {request.program}.
          </p>
          <p class="why">{request.description}</p>
        </div>
      </div>

      <section>
        <h4>With these arguments</h4>
        <!-- The actual thing being approved. Scrolls rather than truncating: a path
             cut in the middle is exactly the path you needed to read. -->
        <pre>{request.arguments}</pre>
      </section>

      {#if queued > 0}
        <p class="queued">{queued} more request{queued === 1 ? '' : 's'} waiting behind this one.</p>
      {/if}
    </div>

    {#snippet footer()}
      <ModalFooter align="between">
        <Toggle bind:checked={remember} label="Allow this tool for the rest of this session" />
        <div class="actions">
          <Button variant="ghost" onclick={deny}>Deny <Kbd keys={["Enter"]} /></Button>
          <Button variant={approveVariant} onclick={allow}>
            Allow once <Kbd keys={["Ctrl", "Enter"]} />
          </Button>
        </div>
      </ModalFooter>
    {/snippet}
  </Modal>
{/if}

<style>
  .body       { display: flex; flex-direction: column; gap: 16px; min-height: 0; }
  .lede       { display: flex; gap: 12px; align-items: flex-start; color: var(--text-primary); }
  .what       { margin: 0 0 4px; font-size: 13px; line-height: 1.5; }
  .tool       { color: var(--text-tertiary); font-family: var(--font-mono); font-size: 11px; }
  .why        { margin: 0; font-size: 12px; line-height: 1.5; color: var(--text-secondary); }

  section     { display: flex; flex-direction: column; gap: 6px; min-height: 0; }
  h4          { margin: 0; font-size: 11px; font-weight: 600; text-transform: uppercase;
                letter-spacing: .04em; color: var(--text-tertiary); }
  pre         { margin: 0; padding: 10px 12px; max-height: 220px; overflow: auto;
                background: var(--bg-base); border: 1px solid var(--border-subtle);
                border-radius: var(--radius-md); font-family: var(--font-mono);
                font-size: 11.5px; line-height: 1.5; color: var(--text-primary);
                white-space: pre-wrap; word-break: break-word; }

  .queued     { margin: 0; font-size: 11.5px; color: var(--text-tertiary); }
  .actions    { display: flex; gap: 8px; align-items: center; }
</style>
