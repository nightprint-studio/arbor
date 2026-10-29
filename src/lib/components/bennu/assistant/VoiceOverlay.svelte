<script lang="ts">
  /**
   * VoiceOverlay — the surface for a spoken request: a darkened scrim over the whole window with Imber in
   * the middle, moving with the voice.
   *
   * It opens when push-to-talk is pressed or the wake word is heard. While you talk the orb swells with the
   * microphone and the words you say appear, large, as they are understood (the tail that has just arrived
   * is dimmer and fades in). When the sentence ends it is sent to Imber; the orb then shrinks into a header
   * and the answer streams into a calm card below. The first follow-up, or "Continue in the panel", closes
   * the overlay and opens the assistant panel on the same conversation.
   *
   * It is an in-window layer, not a native window and not a `Modal`: WebView2 delivers no input to
   * transparent windows, and a dim translucent colour (no blur) is all the "glass" it needs.
   *
   * Keys: Esc cancels what is being said, or closes once there is an answer; Enter sends what has been said
   * so far; Ctrl/Cmd+Enter in the follow-up field sends it.
   */
  import { onMount, tick, untrack } from 'svelte';
  import { fade, fly } from 'svelte/transition';
  import { Mic, FileText, TextCursorInput, PanelRightOpen, Sparkles, X, RotateCcw, Settings } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import LevelWave from '$lib/components/shared/ui/LevelWave.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import ImberOrb, { type ImberOrbState } from '$lib/components/shared/internal/ImberOrb.svelte';
  import ImberAurora from '$lib/components/shared/internal/ImberAurora.svelte';
  import AssistantMessage from './AssistantMessage.svelte';
  import { classifyProblem, markFresh, type TranscriptWord } from './assistant-voice';
  import { animStore } from '$lib/stores/animations.svelte';
  import { voiceStore } from '$lib/stores/voice.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';

  let {
    request,
    problem,
    onClose,
    onSend,
    onRetry,
    onSettings,
    onContinue,
    onFollowUp,
  }: {
    /** The recognised sentence, once there is one. */
    request: string | null;
    /** Why nothing is happening ("no speech was heard", an engine error). */
    problem: string | null;
    onClose: () => void;
    /** Stop listening now and send what has been said. */
    onSend: () => void;
    /** Listen again after nothing was heard. */
    onRetry: () => void;
    /** Open Settings › Voice (no engine, no model, no microphone). */
    onSettings: () => void;
    /** Close the overlay and open the panel on this conversation. */
    onContinue: () => void;
    /** A follow-up typed here: closes the overlay, opens the panel and sends it. */
    onFollowUp: (text: string) => void;
  } = $props();

  const store = bennuAssistantStore;
  /** The orb is laid out at this size and scaled to fit, so resizing it never re-lays-out its insides. */
  const ORB = 188;

  let root = $state<HTMLElement | undefined>();
  let followEl = $state<HTMLTextAreaElement | undefined>();
  let primaryEl = $state<HTMLButtonElement | undefined>();
  let answerEl = $state<HTMLElement | undefined>();
  let answerInner = $state<HTMLElement | undefined>();
  let viewportH = $state(800);
  let follow = $state('');
  let closing = $state(false);

  const fileName = $derived(projectStore.activeFilePath?.split(/[\\/]/).pop() ?? null);
  const selected = $derived(store.selectionLines);

  // ── What is going on ─────────────────────────────────────────────────────────
  type Phase = 'idle' | 'listening' | 'transcribing' | 'answer' | 'problem';
  const phase = $derived<Phase>(
    voiceStore.listening ? 'listening'
      : voiceStore.transcribing ? 'transcribing'
      : request ? 'answer'
      : problem ? 'problem'
      : 'idle',
  );

  const assistantMessages = $derived((store.active?.messages ?? []).filter((m) => m.role !== 'user'));
  const hasAnswerText = $derived(assistantMessages.some((m) => m.blocks.some((b) => b.type === 'text' && b.text.trim())));
  const orbState = $derived<ImberOrbState>(
    phase === 'listening' ? 'listening'
      : phase === 'transcribing' ? 'thinking'
      : phase === 'answer' ? (store.running ? (hasAnswerText ? 'speaking' : 'thinking') : 'idle')
      : 'idle',
  );
  const statusText = $derived(
    phase === 'listening' ? 'Listening…'
      : phase === 'transcribing' ? 'Transcribing…'
      : phase === 'answer' ? (store.running ? (hasAnswerText ? 'Answering…' : 'Thinking…') : 'Imber')
      : phase === 'problem' ? classifyProblem(problem ?? '').title
      : 'Getting ready…',
  );
  /** Once there is an answer the orb is a small mark in the header and the card takes the room. */
  const compact = $derived(phase === 'answer');
  const showWave = $derived(phase === 'listening' || phase === 'transcribing' || phase === 'idle');
  const problemInfo = $derived(problem ? classifyProblem(problem) : null);

  // The orb's slot: big while listening (but never more than a quarter of a short window), a small mark in the header after.
  const slot = $derived(compact ? 56 : Math.max(112, Math.min(ORB, Math.round(viewportH * 0.26))));

  // ── The words ────────────────────────────────────────────────────────────────
  let words = $state<TranscriptWord[]>([]);
  let seen = '';
  $effect(() => {
    const text = voiceStore.liveText;
    untrack(() => { words = markFresh(seen, text); seen = text; });
  });

  const emptyCaption = $derived(
    voiceStore.live || phase === 'idle' ? 'Start speaking…' : (voiceStore.liveNote ?? 'Your words appear when you stop.'),
  );

  // ── Keys it really answers to ────────────────────────────────────────────────
  interface Hint { keys?: string[]; action?: string; text: string }
  const hints = $derived.by<Hint[]>(() => {
    if (phase === 'listening') {
      const out: Hint[] = [{ keys: ['Enter'], text: 'Send now' }, { keys: ['Esc'], text: 'Cancel' }];
      if (voiceStore.reason === 'push_to_talk') out.push({ text: 'Release the keys to send' });
      else if (voiceStore.reason === 'toggle') out.push({ action: 'voice_toggle', text: 'again to send' });
      else if (voiceStore.reason === 'wake') out.push({ text: 'A pause sends it' });
      return out;
    }
    if (phase === 'transcribing') return [{ keys: ['Esc'], text: 'Stop' }];
    if (phase === 'answer') return [{ keys: ['Esc'], text: 'Close' }, { keys: ['Ctrl', 'Enter'], text: 'Send a follow-up' }];
    return [];
  });

  // ── Closing, sending, focus ──────────────────────────────────────────────────
  /** Esc, the scrim and the × all mean the same: stop whatever is happening and go away. */
  function dismiss() {
    if (closing) return;
    if (voiceStore.busy) void voiceStore.cancel();
    onClose();
  }

  function submitFollow() {
    const text = follow.trim();
    if (text) onFollowUp(text);
  }

  const FOCUSABLE = 'button:not([disabled]), textarea:not([disabled]), input:not([disabled]), [href], [tabindex]:not([tabindex="-1"])';

  function trapTab(e: KeyboardEvent) {
    if (!root) return;
    const items = [...root.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.offsetParent !== null);
    const active = document.activeElement as HTMLElement | null;
    if (!items.length) { e.preventDefault(); root.focus(); return; }
    if (!active || !root.contains(active) || active === root) { e.preventDefault(); items[e.shiftKey ? items.length - 1 : 0].focus(); return; }
    if (e.shiftKey && active === items[0]) { e.preventDefault(); items[items.length - 1].focus(); }
    else if (!e.shiftKey && active === items[items.length - 1]) { e.preventDefault(); items[0].focus(); }
  }

  function onKeydown(e: KeyboardEvent) {
    if (closing) return;
    if (e.key === 'Escape') {
      e.preventDefault(); e.stopPropagation();
      dismiss();
    } else if (e.key === 'Tab') {
      trapTab(e);
    } else if (e.key === 'Enter' && phase === 'listening' && !e.repeat) {
      // Enter on a focused button is that button's; anywhere else it means "that is all, send it".
      const tag = (e.target as HTMLElement | null)?.tagName;
      if (tag === 'BUTTON' || tag === 'TEXTAREA' || tag === 'INPUT') return;
      e.preventDefault(); e.stopPropagation();
      onSend();
    }
  }

  // A click on the empty scrim dismisses; one that starts or ends inside a card (a text selection drag) does not.
  let downOutside = false;
  const inside = (t: EventTarget | null) => !!(t as HTMLElement | null)?.closest?.('[data-keep]');
  function onScrimDown(e: MouseEvent) { downOutside = !inside(e.target); }
  function onScrimClick(e: MouseEvent) {
    const dismissIt = downOutside && !inside(e.target);
    downOutside = false;
    if (dismissIt) dismiss();
  }

  onMount(() => {
    const previouslyFocused = document.activeElement as HTMLElement | null;
    return () => { try { previouslyFocused?.focus({ preventScroll: true }); } catch { /* element may be gone */ } };
  });

  // Focus goes where the next keystroke is wanted: the follow-up field once there is an answer, the first
  // button when something went wrong, the dialog itself otherwise (so Enter / Esc reach it).
  $effect(() => {
    const p = phase;
    void tick().then(() => {
      if (p === 'answer') followEl?.focus();
      else if (p === 'problem') primaryEl?.focus();
      else root?.focus();
    });
  });

  // The answer follows its own last line while it is being written, unless the reader scrolled up.
  $effect(() => {
    const box = answerEl;
    const inner = answerInner;
    if (!box || !inner) return;
    let pinned = true;
    const onScroll = () => { pinned = box.scrollHeight - box.scrollTop - box.clientHeight < 60; };
    const ro = new ResizeObserver(() => { if (pinned) box.scrollTop = box.scrollHeight; });
    box.addEventListener('scroll', onScroll, { passive: true });
    ro.observe(inner);
    return () => { ro.disconnect(); box.removeEventListener('scroll', onScroll); };
  });
</script>

<svelte:window bind:innerHeight={viewportH} onkeydowncapture={onKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="scrim"
  class:closing
  bind:this={root}
  role="dialog"
  aria-modal="true"
  aria-label="Imber voice request"
  tabindex="-1"
  onmousedown={onScrimDown}
  onclick={onScrimClick}
  transition:fade={{ duration: animStore.dPanel }}
  onoutrostart={() => { closing = true; }}
>
  <ImberAurora hue={phase === 'problem' ? 'var(--warning)' : 'var(--accent)'} scale={2.4} intensity={0.55} />

  <div class="close">
    <IconButton tooltip="Close" shortcut="Esc" size={30} onclick={dismiss}><X size={16} /></IconButton>
  </div>

  <div class="stage" class:compact>
    <!-- Springs: they share the free height while the orb is the centrepiece and give it up to the answer card. -->
    <div class="spring"></div>

    <!-- The orb: one element that is large while listening and a small mark in the header afterwards. -->
    <div class="orb-slot" style="--slot: {slot}px; --fit: {slot / ORB}">
      <div class="orb-fit">
        <ImberOrb size={ORB} hue={phase === 'problem' ? 'var(--warning)' : 'var(--accent)'} level={voiceStore.level} state={orbState}>
          {#if phase === 'listening'}<Mic size={40} />{:else}<Sparkles size={40} />{/if}
        </ImberOrb>
      </div>
    </div>

    <div class="wave" class:hidden={!showWave} aria-hidden={!showWave}>
      <LevelWave level={voiceStore.level} active={phase === 'listening'} bars={33} height={36} ariaLabel="Microphone level" />
    </div>

    <div class="status" aria-live="polite">
      {#key statusText}
        <span class="status-text" in:fly={{ y: 6, duration: animStore.dBase }} out:fade={{ duration: animStore.dFast }}>{statusText}</span>
      {/key}
    </div>

    {#if phase === 'answer'}
      <p class="said" data-keep>“{request}”</p>
    {:else if phase === 'problem' && problemInfo}
      <div class="problem" data-keep>
        <p class="problem-detail">{problemInfo.detail}</p>
        <div class="problem-actions">
          {#if problemInfo.kind === 'setup'}
            <Button variant="primary" size="md" onclick={onSettings} bind:element={primaryEl}>
              {#snippet iconStart()}<Settings size={14} />{/snippet}
              Open Voice settings
            </Button>
          {:else}
            <Button variant="primary" size="md" onclick={onRetry} bind:element={primaryEl}>
              {#snippet iconStart()}<RotateCcw size={14} />{/snippet}
              Try again
            </Button>
          {/if}
          <Button variant="ghost" size="md" onclick={dismiss}>Close</Button>
        </div>
      </div>
    {:else}
      <div class="transcript" aria-label="What was heard">
        {#if words.length}
          <p class="words">{#each words as w, i (i)}<span class="w" class:fresh={w.fresh}>{w.text}{' '}</span>{/each}</p>
        {:else}
          <p class="words empty">{emptyCaption}</p>
        {/if}
      </div>
    {/if}

    {#if fileName || selected > 0}
      <div class="chips" data-keep>
        {#if fileName}<Badge variant="chip" size="sm" label={fileName}>{#snippet icon()}<FileText size={10} />{/snippet}</Badge>{/if}
        {#if selected > 0}<Badge variant="chip" size="sm" label={selected === 1 ? '1 line selected' : `${selected} lines selected`}>{#snippet icon()}<TextCursorInput size={10} />{/snippet}</Badge>{/if}
      </div>
    {/if}

    {#if phase === 'answer'}
      <div class="card" data-keep in:fly={{ y: 14, duration: animStore.dPanel }}>
        <div class="answer" bind:this={answerEl}>
          <div class="answer-inner" bind:this={answerInner}>
            {#each assistantMessages as m (m.id)}
              <AssistantMessage message={m} conversationId={store.active?.id ?? ''} mode={store.mode} last={m.id === assistantMessages.at(-1)?.id} />
            {/each}
          </div>
        </div>
        <div class="dock">
          <div class="follow">
            <Textarea bind:value={follow} bind:element={followEl} placeholder="Follow up… (continues in the panel)" ariaLabel="Follow-up" minRows={1} maxRows={4} onsubmit={submitFollow} />
          </div>
          <Button variant="primary" size="md" onclick={onContinue}>
            {#snippet iconStart()}<PanelRightOpen size={14} />{/snippet}
            Continue in the panel
          </Button>
          <Button variant="ghost" size="md" onclick={dismiss}>Close</Button>
        </div>
      </div>
    {/if}

    {#if hints.length}
      <div class="hints">
        {#each hints as h, i (i)}
          <span class="hint">
            {#if h.action}<Kbd action={h.action} size="sm" />{:else if h.keys}<Kbd keys={h.keys} size="sm" />{/if}
            <span>{h.text}</span>
          </span>
        {/each}
      </div>
    {/if}

    <div class="spring"></div>
  </div>
</div>

<style>
  .scrim {
    position: fixed; inset: 0; z-index: var(--z-modal-bg);
    display: flex; align-items: center; justify-content: center;
    padding: 24px 16px; overflow: hidden; isolation: isolate;
    /* A plain translucent colour: the app stays visible underneath, dimmed. No blur — it would keep the
       WebView2 compositor busy for as long as the overlay is open. */
    background: color-mix(in srgb, color-mix(in srgb, var(--bg-base) 70%, #000) 84%, transparent);
  }
  .scrim:focus { outline: none; }
  .scrim.closing { pointer-events: none; }
  .close { position: absolute; top: 14px; right: 16px; z-index: 2; }

  .stage {
    position: relative; display: flex; flex-direction: column; align-items: center; gap: 14px;
    width: min(720px, 100%); height: min(100%, 780px); min-height: 0;
    animation: stage-in calc(var(--anim-dur-panel, 200ms) * 1.4) var(--anim-easing-spring, ease-out) both;
  }
  .stage > * { flex-shrink: 0; }
  /* The free height: shared above and below the orb at first, then handed to the answer card. */
  .spring { flex: 1 1 0; min-height: 0; transition: flex-grow calc(var(--anim-dur-panel, 200ms) * 1.6) var(--anim-easing-spring, ease); }
  .compact .spring { flex-grow: 0; }
  .compact { gap: 12px; padding-top: 8px; }

  /* ── The orb ─────────────────────────────────────────────────────────────── */
  .orb-slot {
    position: relative; width: var(--slot); height: var(--slot);
    transition: width calc(var(--anim-dur-panel, 200ms) * 1.6) var(--anim-easing-spring, ease),
                height calc(var(--anim-dur-panel, 200ms) * 1.6) var(--anim-easing-spring, ease);
  }
  .orb-fit {
    position: absolute; left: 50%; top: 50%; width: 188px; height: 188px; margin: -94px 0 0 -94px;
    transform: scale(var(--fit)); will-change: transform;
    transition: transform calc(var(--anim-dur-panel, 200ms) * 1.6) var(--anim-easing-spring, ease);
  }

  .wave { height: 36px; overflow: hidden; transition: height var(--anim-dur-panel, 200ms) ease, opacity var(--anim-dur-panel, 200ms) ease; }
  .wave.hidden { height: 0; opacity: 0; margin-top: -14px; }

  /* ── Status line: the words cross-fade in place ─────────────────────────── */
  .status { display: grid; justify-items: center; min-height: 1.8em; }
  .status-text {
    grid-area: 1 / 1; font-size: var(--font-size-xl); font-weight: 600; letter-spacing: -0.01em; color: var(--text-primary);
  }
  .compact .status-text { font-size: var(--font-size-md); color: var(--text-secondary); }

  /* ── What was heard ─────────────────────────────────────────────────────── */
  .transcript {
    width: min(640px, 100%); min-height: 3.4em; max-height: 6.2em; overflow: hidden;
    display: flex; flex-direction: column; justify-content: flex-end;
    -webkit-mask-image: linear-gradient(to bottom, transparent, #000 38%);
    mask-image: linear-gradient(to bottom, transparent, #000 38%);
  }
  .words {
    margin: 0; text-align: center; overflow-wrap: anywhere;
    font-size: clamp(1.15rem, 2.6vw, 1.7rem); line-height: 1.4; font-weight: 550; color: var(--text-primary);
  }
  .words.empty { font-weight: 450; color: var(--text-muted); }
  .w { color: var(--text-primary); transition: color calc(var(--anim-dur-slow, 240ms) * 1.5) ease; }
  .w.fresh { color: var(--text-secondary); animation: word-in calc(var(--anim-dur-slow, 240ms) * 1.2) ease-out; }

  .said {
    margin: 0; max-width: 560px; text-align: center; font-size: var(--font-size-md); line-height: 1.45; color: var(--text-primary);
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
  }

  .chips { display: flex; flex-wrap: wrap; justify-content: center; gap: 6px; }

  /* ── Something went wrong ───────────────────────────────────────────────── */
  .problem { display: flex; flex-direction: column; align-items: center; gap: 14px; max-width: 460px; }
  .problem-detail { margin: 0; text-align: center; font-size: var(--font-size-sm); line-height: 1.55; color: var(--text-secondary); }
  .problem-actions { display: flex; gap: 8px; flex-wrap: wrap; justify-content: center; }

  /* ── The answer ─────────────────────────────────────────────────────────── */
  .card {
    width: 100%; min-height: 0; flex: 1 1 0; display: flex; flex-direction: column;
    background: var(--bg-base); border: 1px solid var(--border-subtle); border-radius: var(--radius-lg);
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.45); overflow: hidden;
  }
  .answer { flex: 1; min-height: 0; overflow-y: auto; padding: 14px 16px 6px; }
  .answer-inner { display: flex; flex-direction: column; gap: 12px; }
  .dock { display: flex; align-items: flex-end; gap: 8px; flex-wrap: wrap; padding: 10px 12px; border-top: 1px solid var(--border-subtle); background: var(--modal-chrome-bg, var(--bg-elevated)); }
  .follow { flex: 1 1 220px; min-width: 0; }
  .follow :global(textarea) { width: 100%; }

  .hints { display: flex; flex-wrap: wrap; justify-content: center; gap: 6px 16px; font-size: var(--font-size-xs); color: var(--text-muted); }
  .hint { display: inline-flex; align-items: center; gap: 6px; }

  @keyframes stage-in {
    from { opacity: 0; transform: translateY(10px) scale(0.98); }
    to   { opacity: 1; transform: none; }
  }
  @keyframes word-in {
    from { opacity: 0; transform: translateY(4px); }
    to   { opacity: 1; transform: none; }
  }

  @media (prefers-reduced-motion: reduce) {
    .stage, .w.fresh { animation: none; }
    .orb-slot, .orb-fit, .wave, .w { transition: none; }
  }
</style>
