<script module lang="ts">
  /** One step of a tour: the id its content switches on, and the label the stepper shows. */
  export interface OnboardingStep {
    id: string;
    label: string;
  }
</script>

<script lang="ts">
  /**
   * The welcome-tour dialog, for any product that has one.
   *
   * ## What it owns
   *
   * The parts that are the same tour whichever product is being introduced: the stepper in the
   * header, the Back / Next / Skip footer and what each button means at the ends of the range,
   * the keyboard contract (Enter and Alt+arrows), the confirm before abandoning it, and the
   * body's visual vocabulary — the hero, the step header, the card grids. A product supplies
   * the steps and the markup of each one.
   *
   * ## Why the vocabulary is `:global`
   *
   * The classes below (`.hero`, `.step-section`, `.three-up`, …) are written in the consumer's
   * snippet, which is a different component, so Svelte's scoping would strip them. Same
   * arrangement `DocsShell` already uses for the documentation vocabulary, and for the same
   * reason: the shell defines what a page of it looks like, and the pages are written elsewhere.
   *
   * ## What it deliberately does not own
   *
   * Completion state. `onFinish` fires when the tour ends — by finishing it or by skipping it —
   * and the product decides what that means. Corvus and Bennu have separate flags: finishing
   * one is no reason to stop introducing the other.
   */
  import { onMount, type Snippet } from 'svelte';
  import { ArrowLeft, ArrowRight, Check, Sparkles } from 'lucide-svelte';

  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import ModalFooter from './ModalFooter.svelte';
  import ConfirmModal from './ConfirmModal.svelte';
  import Button from './ui/Button.svelte';
  import StepIndicator from './ui/StepIndicator.svelte';

  let {
    steps,
    title,
    skipTitle = 'Skip the welcome tour?',
    skipMessage = 'You can re-open it any time from the Command Palette or the Documentation panel.',
    content,
    onFinish,
  }: {
    steps: OnboardingStep[];
    /** Header title, e.g. "Welcome to Bennu". */
    title: string;
    skipTitle?: string;
    skipMessage?: string;
    /** The body of the current step. Receives the step id to switch on. */
    content: Snippet<[string]>;
    /** The tour is over — finished or skipped. */
    onFinish: () => void;
  } = $props();

  let stepIdx = $state(0);
  const step = $derived(steps[stepIdx] ?? steps[0]);
  const isFirst = $derived(stepIdx === 0);
  const isLast = $derived(stepIdx === steps.length - 1);

  /** The confirm sits on top of this modal while set. */
  let skipConfirm = $state(false);

  function next() {
    if (isLast) { onFinish(); return; }
    stepIdx = Math.min(stepIdx + 1, steps.length - 1);
  }
  function back() {
    stepIdx = Math.max(stepIdx - 1, 0);
  }
  function requestSkip() {
    // No prompt on the first step — there is nothing to lose yet.
    if (isFirst) { onFinish(); return; }
    skipConfirm = true;
  }

  // Enter / Ctrl+Enter → Next, Alt+← / Alt+→ → Back / Next. Esc is deliberately not bound:
  // `Modal` already owns it through its stack and routes it to `onClose`, which is the skip.
  function onKey(e: KeyboardEvent) {
    if (skipConfirm) return;
    const target = e.target as HTMLElement | null;
    const tag = target?.tagName ?? '';
    const inField = tag === 'INPUT' || tag === 'TEXTAREA';
    if (e.key === 'Enter') {
      if (inField && !(e.ctrlKey || e.metaKey)) return;
      e.preventDefault();
      next();
      return;
    }
    if (e.altKey && e.key === 'ArrowRight') { e.preventDefault(); next(); return; }
    if (e.altKey && e.key === 'ArrowLeft') { e.preventDefault(); back(); return; }
  }

  onMount(() => {
    // Back to the start on every open, so a re-run from the palette does not resume where a
    // previous dismissal left off.
    stepIdx = 0;
  });
</script>

<svelte:window onkeydown={onKey} />

{#snippet backIcon()}<ArrowLeft size={14} />{/snippet}
{#snippet nextIcon()}<ArrowRight size={14} />{/snippet}
{#snippet checkIcon()}<Check size={14} strokeWidth={3} />{/snippet}

<Modal
  onClose={requestSkip}
  width="720px"
  height="560px"
  padBody={false}
  ariaLabel={title}
  zIndex="var(--z-onboarding)"
>
  {#snippet header()}
    <ModalHeader onClose={requestSkip}>
      <Sparkles size={14} class="hdr-icon" />
      <span class="modal-title">{title}</span>
      <span class="header-steps">
        <StepIndicator
          {steps}
          current={step.id}
          variant="pill"
          collapseLabels
          onStepClick={(_id, i) => { stepIdx = i; }}
        />
      </span>
    </ModalHeader>
  {/snippet}

  <!-- One section per step. The body owns its own padding so a step can opt into a full-bleed
       hero wash. -->
  <div class="ob-body" data-step={step.id}>
    {@render content(step.id)}
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <Button variant="ghost" onclick={requestSkip}>
        {isLast ? 'Close' : 'Skip tour'}
      </Button>
      <div class="footer-right">
        <Button variant="secondary" onclick={back} disabled={isFirst} iconStart={backIcon}>
          Back
        </Button>
        {#if isLast}
          <Button variant="primary" onclick={onFinish} iconStart={checkIcon}>
            Finish
          </Button>
        {:else}
          <Button variant="primary" onclick={next} iconEnd={nextIcon}>
            Next
          </Button>
        {/if}
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

{#if skipConfirm}
  <ConfirmModal
    title={skipTitle}
    message={skipMessage}
    confirmLabel="Skip"
    cancelLabel="Keep going"
    variant="info"
    onConfirm={() => { skipConfirm = false; onFinish(); }}
    onCancel={() => (skipConfirm = false)}
  />
{/if}

<style>

  /* ── Header ──────────────────────────────────────────────────────────── */
  :global(.hdr-icon) { color: var(--accent); flex-shrink: 0; }

  /* The StepIndicator floats to the right of the modal title. Pushing it
     here (instead of inside the widget) keeps StepIndicator generic — it
     doesn't know whether it lives in a modal header, a sidebar panel, or
     a settings page. */
  .header-steps {
    margin-left: auto;
    display: flex;
    align-items: center;
    min-width: 0;
  }

  /* ── Body shell ────────────────────────────────────────────────────────── */
  .ob-body {
    height: 100%;
    overflow: auto;
    padding: 28px 32px;
    background:
      radial-gradient(120% 60% at 0% 0%, color-mix(in srgb, var(--accent) 10%, transparent), transparent 70%),
      radial-gradient(80% 60% at 100% 100%, color-mix(in srgb, var(--accent) 6%, transparent), transparent 70%),
      var(--bg-base);
  }

  /* ── Hero (welcome + finish) ──────────────────────────────────────────── */
  :global(.hero) {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    height: 100%;
    gap: 14px;
    padding: 4px;
  }
  :global(.hero-logo) {
    width: 88px;
    height: 88px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 24px;
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--accent) 35%, transparent),
      0 14px 40px -10px color-mix(in srgb, var(--accent) 50%, transparent);
  }
  :global(.hero h1) {
    margin: 4px 0 0;
    font-size: 30px;
    font-weight: 600;
    letter-spacing: -0.5px;
  }
  :global(.hero .tagline) {
    color: var(--text-secondary);
    font-size: var(--font-size-md);
    margin: 0 0 6px;
    max-width: 460px;
    line-height: 1.45;
  }
  /* Pillars list — IconCard does the visual work, this is just the layout
     container (vertical stack, max-width to keep the hero readable). */
  :global(.pillars) {
    list-style: none;
    margin: 18px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 100%;
    max-width: 460px;
    text-align: left;
  }
  :global(.pillars li) { display: contents; }

  :global(.finish .finish-mark) {
    width: 88px;
    height: 88px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--success) 18%, transparent);
    color: var(--success);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--success) 45%, transparent),
      0 14px 40px -10px color-mix(in srgb, var(--success) 55%, transparent);
  }
  :global(.finish-links) {
    display: flex;
    gap: 10px;
    margin-top: 12px;
  }

  /* ── Generic step section ─────────────────────────────────────────────── */
  :global(.step-section) {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  :global(.step-header) {
    display: flex;
    align-items: flex-start;
    gap: 14px;
  }
  :global(.step-header h2) {
    margin: 2px 0 4px;
    font-size: var(--font-size-2xl);
    font-weight: 600;
    letter-spacing: -0.2px;
  }
  :global(.step-header p) {
    margin: 0;
    color: var(--text-secondary);
    font-size: var(--font-size-sm);
    line-height: 1.5;
    max-width: 560px;
  }
  :global(.step-icon) {
    width: 44px;
    height: 44px;
    border-radius: 12px;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: var(--accent-subtle);
    color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
  }
  :global(.step-icon.teaser-icon) {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }
  :global(.step-header p code) {
    font-family: var(--font-code);
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: 4px;
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    font-size: 0.9em;
  }

  /* ── Three-up grid (provider tiles + first-repo tiles) ───────────────── */
  /* Both the provider teaser row and the open/clone/init picker render
     three IconCards side by side; this is the only layout still owned
     here, the tiles themselves are stock IconCard. */
  :global(.three-up) {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 12px;
  }

  /* ── Feature list (power-features step) ──────────────────────────────── */
  /* Same trick as `.pillars`: the list element is structural, IconCard
     paints the row. */
  :global(.feature-list) {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  :global(.feature-list li) { display: contents; }

  /* Issue-tracker description carries an inline accent emphasis ("click it
     to open the issue instantly"). Lives here because IconCard's stock
     `description` is plain text — when the copy needs richer markup we
     pass it via the `extra` snippet, and this rule styles it. */
  :global(.feat-desc-rich) {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    line-height: 1.5;
  }
  :global(.feat-desc-rich em) {
    font-style: normal;
    color: var(--accent);
    font-weight: 500;
  }

  /* ── Footer ───────────────────────────────────────────────────────────── */
  .footer-right { display: flex; gap: 8px; }
</style>
