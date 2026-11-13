<script lang="ts">
  /**
   * Where you write to Imber — the frame every product's composer shares.
   *
   * One frame holds the next message: what it will be given (chips), attachments, the text, and one quiet
   * line of controls with the send / stop button at the right edge; under the frame a foot line with a hint
   * and the figures that matter (cost, limit). The frame owns the look and the keyboard contract (Ctrl+Enter
   * sends, drop highlights); the product owns what goes in it, through the snippets: it passes its own
   * model/mode menus, its own chips, its attachment strip, its slash-command list.
   */
  import type { Snippet } from 'svelte';
  import { ArrowUp, Square } from 'lucide-svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import './imber-motion.css';

  let {
    value = $bindable(''),
    element = $bindable(),
    placeholder = 'Ask Imber…',
    disabled = false,
    running = false,
    canSend = false,
    sendWhileRunning = false,
    sendTooltip = 'Send',
    dropping = false,
    chips,
    attachments,
    beforeField,
    popup,
    controls,
    trailing,
    sendIcon,
    hint,
    figures,
    onSend,
    onStop,
    onfocusin,
    ondragover,
    ondragleave,
    ondrop,
    onfieldkeydown,
    onfieldpaste,
  }: {
    value?: string;
    /** The text field, for a product that moves the focus or the caret. */
    element?: HTMLTextAreaElement;
    placeholder?: string;
    /** The field is read-only for now (an approval is waiting). */
    disabled?: boolean;
    /** A run is open: the send button becomes Stop. */
    running?: boolean;
    canSend?: boolean;
    /** While it runs, a message can still be sent: Imber reads it after the step it is in. The product's
     *  `onSend` decides what sending means then. */
    sendWhileRunning?: boolean;
    sendTooltip?: string;
    /** Something is being dragged over the frame. */
    dropping?: boolean;
    /** What the message will carry besides its text, as chips. */
    chips?: Snippet;
    attachments?: Snippet;
    /** Anything shown between the attachments and the field (fenced blocks pulled out of the text). */
    beforeField?: Snippet;
    /** A list that opens upward over the thread from the field (slash commands). */
    popup?: Snippet;
    /** Left side of the control line: model, settings, attach, microphone. */
    controls?: Snippet;
    /** Right side, before the send button: the mode menu. */
    trailing?: Snippet;
    /** Replaces the arrow on the send button (the eye of a dry run). */
    sendIcon?: Snippet;
    hint?: Snippet;
    /** Right side of the foot line: limit, cost. */
    figures?: Snippet;
    onSend: () => void;
    onStop?: () => void;
    onfocusin?: (e: FocusEvent) => void;
    ondragover?: (e: DragEvent) => void;
    ondragleave?: (e: DragEvent) => void;
    ondrop?: (e: DragEvent) => void;
    onfieldkeydown?: (e: KeyboardEvent) => void;
    onfieldpaste?: (e: ClipboardEvent) => void;
  } = $props();
</script>

<div class="composer">
  <div class="frame" class:dropping role="presentation" {onfocusin} {ondragover} {ondragleave} {ondrop}>
    {#if chips}<div class="chips">{@render chips()}</div>{/if}
    {@render attachments?.()}
    {@render beforeField?.()}

    <!-- Keys and paste are handled on the way up from the textarea, which keeps the focus. -->
    <div class="field" role="presentation" onkeydown={onfieldkeydown} onpaste={onfieldpaste}>
      {#if popup}<div class="popup">{@render popup()}</div>{/if}
      <Textarea
        bind:value
        bind:element
        {placeholder}
        {disabled}
        minRows={2}
        maxRows={9}
        ariaLabel="Message to Imber"
        onsubmit={onSend}
      />
    </div>

    <div class="controls">
      {@render controls?.()}
      <span class="grow"></span>
      {@render trailing?.()}
      {#if running}
        {#if sendWhileRunning && canSend}
          <IconButton tooltip="Send now — read after the current step" shortcut="Ctrl+Enter" variant="ghost" size={28} disabled={disabled} onclick={onSend}>
            <ArrowUp size={15} />
          </IconButton>
        {/if}
        <span class="asst-ring"><IconButton tooltip="Stop" shortcut="Esc" variant="accent" size={28} onclick={onStop}><Square size={12} /></IconButton></span>
      {:else}
        <IconButton tooltip={sendTooltip} shortcut="Ctrl+Enter" variant="accent" size={28} disabled={!canSend || disabled} onclick={onSend}>
          {#if sendIcon}{@render sendIcon()}{:else}<ArrowUp size={15} />{/if}
        </IconButton>
      {/if}
    </div>
  </div>

  {#if hint || figures}
    <div class="foot">
      <span class="hint">{@render hint?.()}</span>
      {@render figures?.()}
    </div>
  {/if}
</div>

<style>
  .composer { display: flex; flex-direction: column; gap: 6px; padding: 8px 10px 8px; }

  .frame.dropping { border-color: var(--accent-primary); box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent-primary) 30%, transparent); }
  .frame {
    display: flex; flex-direction: column; gap: 8px;
    padding: 9px 10px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg-elevated);
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }
  .frame:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-subtle);
  }

  .chips { display: flex; align-items: center; flex-wrap: wrap; gap: 5px; }
  .chips:empty { display: none; }
  .field { position: relative; min-width: 0; }
  /* Opens upward, over the thread: the composer sits at the bottom of the panel. */
  .popup { position: absolute; left: -4px; right: -4px; bottom: calc(100% + 8px); z-index: 20; }

  .controls { display: flex; align-items: center; gap: 4px; min-width: 0; }
  .grow { flex: 1; min-width: 4px; }

  .foot { display: flex; align-items: center; gap: 8px; padding: 0 4px; min-height: 16px; }
  .hint {
    display: inline-flex; align-items: center; gap: 6px; min-width: 0;
    font-size: var(--font-size-2xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
</style>
