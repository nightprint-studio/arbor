<script lang="ts">
  /**
   * Settings → "Keyboard Inputs" panel.
   *
   * Master switch + visual position picker (mock window with the five
   * anchors as live mini cards) + size, tone, opacity, edge offset,
   * duration and behaviour toggles.  An always-visible live preview
   * card mirrors every setting so the user sees the result before
   * leaving the panel.
   *
   * Pairs with `keystrokesStore` and the global `toggle_keystrokes`
   * keybinding wired in AppShell.
   */
  import {
    Keyboard, MousePointerClick, Filter, Type, Repeat,
    Tag, AlignJustify,
  } from 'lucide-svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Kbd from '$lib/components/shared/ui/Kbd.svelte';
  import {
    keystrokesStore, TONE_COLORS,
    type KeystrokePosition, type KeystrokeSize, type KeystrokeTone,
  } from '$lib/stores/keystrokes.svelte';

  // The store is the source of truth — wire up via $effect so two-way
  // bindings stay in sync if another part of the UI flips the setting.
  let enabled         = $state(keystrokesStore.enabled);
  let position        = $state<KeystrokePosition>(keystrokesStore.position);
  let size            = $state<KeystrokeSize>(keystrokesStore.size);
  let tone            = $state<KeystrokeTone>(keystrokesStore.tone);
  let displayMs       = $state(keystrokesStore.displayMs);
  let opacity         = $state(keystrokesStore.opacity);
  let edgeOffset      = $state(keystrokesStore.edgeOffset);
  let onlyShortcuts   = $state(keystrokesStore.onlyShortcuts);
  let showInInputs    = $state(keystrokesStore.showInInputs);
  let showMouseClicks = $state(keystrokesStore.showMouseClicks);
  let groupRepeats    = $state(keystrokesStore.groupRepeats);
  let showAction      = $state(keystrokesStore.showAction);
  let compact         = $state(keystrokesStore.compact);

  $effect(() => { keystrokesStore.setEnabled(enabled); });
  $effect(() => { keystrokesStore.setPosition(position); });
  $effect(() => { keystrokesStore.setSize(size); });
  $effect(() => { keystrokesStore.setTone(tone); });
  $effect(() => { keystrokesStore.setDisplayMs(displayMs); });
  $effect(() => { keystrokesStore.setOpacity(opacity); });
  $effect(() => { keystrokesStore.setEdgeOffset(edgeOffset); });
  $effect(() => { keystrokesStore.setOnlyShortcuts(onlyShortcuts); });
  $effect(() => { keystrokesStore.setShowInInputs(showInInputs); });
  $effect(() => { keystrokesStore.setShowMouseClicks(showMouseClicks); });
  $effect(() => { keystrokesStore.setGroupRepeats(groupRepeats); });
  $effect(() => { keystrokesStore.setShowAction(showAction); });
  $effect(() => { keystrokesStore.setCompact(compact); });

  // The five anchor spots, rendered as buttons over a mock window.
  const ANCHORS: { id: KeystrokePosition; label: string }[] = [
    { id: 'top-left',      label: 'Top Left'      },
    { id: 'top-right',     label: 'Top Right'     },
    { id: 'bottom-left',   label: 'Bottom Left'   },
    { id: 'bottom-center', label: 'Bottom Center' },
    { id: 'bottom-right',  label: 'Bottom Right'  },
  ];

  const SIZE_OPTIONS = [
    { value: 'sm', label: 'Small'  },
    { value: 'md', label: 'Medium' },
    { value: 'lg', label: 'Large'  },
  ];

  const TONE_OPTIONS: { value: KeystrokeTone; label: string }[] = [
    { value: 'accent', label: 'Accent' },
    { value: 'neon',   label: 'Neon'   },
    { value: 'aqua',   label: 'Aqua'   },
    { value: 'amber',  label: 'Amber'  },
    { value: 'mono',   label: 'Mono'   },
  ];

  // Resolve the CSS colour applied to swatches and preview accents.
  const toneColor = $derived(
    tone === 'accent' ? 'var(--accent)'
    : tone === 'mono' ? 'var(--text-primary)'
    : TONE_COLORS[tone]
  );

  // ── Preset preview launcher ────────────────────────────────────────────
  function preview(chord: string[], action: string | null = null) {
    // Make sure the store is on, otherwise it discards the entry.
    if (!enabled) keystrokesStore.setEnabled(true);
    keystrokesStore.recordChord(chord, action);
  }

  function isModifier(p: string): boolean {
    return p === 'Ctrl' || p === 'Alt' || p === 'Shift' || p === 'Meta';
  }

  // Sample chord shown by the always-on live preview card.
  const SAMPLE_PARTS  = ['Ctrl', 'Shift', 'K'];
  const SAMPLE_ACTION = 'Open Command Palette';
</script>

<SectionHeader
  title="Keyboard Inputs"
  description="Display a floating overlay of every key, chord and (optionally) mouse click — paired with the human-readable action each shortcut triggers. Perfect for demos, screencasts and pair-programming."
/>

<div class="card">
  <FormRow
    label="Show keyboard inputs"
    description="Render a floating overlay of recent key presses. Toggle anytime with the global shortcut — even from inside a modal."
  >
    <div class="enable-row">
      <Kbd action="toggle_keystrokes" tone="muted" size="sm" />
      <Toggle bind:checked={enabled} />
    </div>
  </FormRow>
</div>

<!-- Once enabled, expose all the customisation knobs. -->
{#if enabled}
  <!-- ── Position picker ──────────────────────────────────────────────── -->
  <div class="card">
    <div class="card-head">
      <div>
        <div class="card-title">Position</div>
        <div class="card-sub">Where the overlay anchors on screen</div>
      </div>
      <span class="badge">{ANCHORS.find(a => a.id === position)?.label ?? ''}</span>
    </div>

    <div
      class="window-mock"
      role="radiogroup"
      aria-label="Overlay position"
      style="--tone: {toneColor};"
    >
      <div class="window-titlebar">
        <span class="dot red"></span>
        <span class="dot amber"></span>
        <span class="dot green"></span>
      </div>
      <div class="window-body">
        {#each ANCHORS as anchor}
          <button
            type="button"
            class="anchor pos-{anchor.id}"
            class:selected={position === anchor.id}
            aria-pressed={position === anchor.id}
            aria-label={anchor.label}
            title={anchor.label}
            onclick={() => (position = anchor.id)}
          >
            <span class="anchor-mini">
              <span class="anchor-stripe"></span>
              <span class="anchor-dots">
                <i></i><i></i><i></i>
              </span>
            </span>
          </button>
        {/each}
      </div>
    </div>

    <FormRow label="Edge offset" description="Distance from the anchored screen edge ({edgeOffset} px)">
      <input
        class="slider"
        type="range"
        min="8" max="120" step="2"
        bind:value={edgeOffset}
        aria-label="Edge offset"
      />
    </FormRow>
  </div>

  <!-- ── Live preview ─────────────────────────────────────────────────── -->
  <div class="card preview-card">
    <div class="card-head">
      <div>
        <div class="card-title">Preview</div>
        <div class="card-sub">A live mock of your current overlay style</div>
      </div>
    </div>
    <div
      class="preview-stage"
      style="--tone: {toneColor}; --preview-opacity: {opacity};"
    >
      <div class="ks-card sz-{size}" class:compact class:has-action={showAction}>
        <span class="ks-stripe"></span>
        <div class="ks-body">
          <div class="ks-chord-row">
            <span class="ks-chord">
              {#each SAMPLE_PARTS as part, i}
                {#if i > 0}<span class="ks-plus">+</span>{/if}
                <kbd class="ks-key" class:mod={isModifier(part)}>{part}</kbd>
              {/each}
            </span>
            <span class="ks-count">×2</span>
          </div>
          {#if showAction}
            <div class="ks-action">{SAMPLE_ACTION}</div>
          {/if}
        </div>
      </div>
    </div>
  </div>

  <!-- ── Appearance ───────────────────────────────────────────────────── -->
  <div class="card">
    <FormRow label="Size" description="Overall scale of each key pill">
      <RadioGroup bind:value={size} options={SIZE_OPTIONS} appearance="segment" size="md" />
    </FormRow>

    <FormRow label="Accent tone" description="Colour applied to the side stripe, modifier keys and ×N counter">
      <div class="tone-row">
        {#each TONE_OPTIONS as opt}
          <button
            type="button"
            class="tone-swatch"
            class:selected={tone === opt.value}
            aria-pressed={tone === opt.value}
            aria-label={opt.label}
            title={opt.label}
            onclick={() => (tone = opt.value)}
            style="--swatch: {opt.value === 'accent' ? 'var(--accent)'
                          : opt.value === 'mono'   ? 'var(--text-primary)'
                          : TONE_COLORS[opt.value]}"
          >
            <span class="tone-dot"></span>
            <span class="tone-name">{opt.label}</span>
          </button>
        {/each}
      </div>
    </FormRow>

    <FormRow label="Visibility" description="How long each keystroke stays on screen ({(displayMs / 1000).toFixed(1)}s)">
      <input
        class="slider"
        type="range"
        min="500" max="6000" step="100"
        bind:value={displayMs}
        aria-label="Display duration"
      />
    </FormRow>

    <FormRow label="Opacity" description="Background transparency of each pill ({Math.round(opacity * 100)}%)">
      <input
        class="slider"
        type="range"
        min="0.4" max="1" step="0.05"
        bind:value={opacity}
        aria-label="Overlay opacity"
      />
    </FormRow>

    <FormRow label="Compact layout" description="Put the chord and action on a single line instead of stacking them vertically.">
      <div class="row-icon"><AlignJustify size={14} /><Toggle bind:checked={compact} /></div>
    </FormRow>

    <FormRow label="Show action label" description="Display the name of the action each shortcut triggers (e.g. Ctrl+K → Command palette). Off makes the overlay purely typographic.">
      <div class="row-icon"><Tag size={14} /><Toggle bind:checked={showAction} /></div>
    </FormRow>
  </div>

  <!-- ── Behaviour ────────────────────────────────────────────────────── -->
  <div class="card">
    <FormRow label="Only show shortcuts" description="Hide plain printable keys — only chords using Ctrl, Alt, Shift or Meta will appear.">
      <div class="row-icon"><Filter size={14} /><Toggle bind:checked={onlyShortcuts} /></div>
    </FormRow>

    <FormRow label="Capture while typing" description="Also show keys pressed inside text fields. Off by default to keep the overlay quiet while you write commit messages.">
      <div class="row-icon"><Type size={14} /><Toggle bind:checked={showInInputs} /></div>
    </FormRow>

    <FormRow label="Show mouse clicks" description="Add a small badge for each Left / Middle / Right mouse click.">
      <div class="row-icon"><MousePointerClick size={14} /><Toggle bind:checked={showMouseClicks} /></div>
    </FormRow>

    <FormRow label="Group rapid repeats" description="Collapse the same chord pressed multiple times in a row into a single pill with a ×N counter.">
      <div class="row-icon"><Repeat size={14} /><Toggle bind:checked={groupRepeats} /></div>
    </FormRow>
  </div>

  <!-- ── Try it out ───────────────────────────────────────────────────── -->
  <div class="card try-card">
    <div class="try-head">
      <Keyboard size={14} />
      <span>Try it out</span>
      <span class="try-hint">— hit any key, or click a preset:</span>
    </div>
    <div class="try-buttons">
      <button type="button" class="preset" onclick={() => preview(['Ctrl', 'K'],          'Command palette')}>Ctrl + K</button>
      <button type="button" class="preset" onclick={() => preview(['Ctrl', 'Shift', 'P'], 'Push current branch')}>Ctrl + Shift + P</button>
      <button type="button" class="preset" onclick={() => preview(['Alt', '↑'],            null)}>Alt + ↑</button>
      <button type="button" class="preset" onclick={() => preview(['Esc'],                'Close panel · modal · search')}>Esc</button>
      <button type="button" class="preset" onclick={() => preview(['F5'],                  'Refresh graph (fetch)')}>F5</button>
    </div>
  </div>
{/if}

<style>
  /* ── Generic card-head row ───────────────────────────────────────────── */
  .card-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 10px;
  }
  .card-title {
    color: var(--text-primary);
    font-weight: 600;
    font-size: var(--font-size-md);
  }
  .card-sub {
    color: var(--text-muted);
    font-size: var(--font-size-sm);
    margin-top: 2px;
  }
  .badge {
    color: var(--text-muted);
    font-size: var(--font-size-sm);
    padding: 2px 10px;
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    background: color-mix(in srgb, var(--accent) 6%, transparent);
  }

  .enable-row {
    display: inline-flex;
    align-items: center;
    gap: 12px;
  }

  /* ── Position picker — mini-window mock ──────────────────────────────── */
  .window-mock {
    position: relative;
    margin: 6px 0 14px;
    aspect-ratio: 16 / 9;
    max-width: 460px;
    background-color: var(--bg-base);
    background-image:
      radial-gradient(ellipse at 30% 20%,
        color-mix(in srgb, var(--tone) 14%, transparent),
        transparent 60%),
      linear-gradient(135deg, var(--bg-elevated), var(--bg-base));
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
    box-shadow: 0 10px 26px -12px rgba(0, 0, 0, 0.45);
  }

  .window-titlebar {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 22px;
    padding: 0 10px;
    background: color-mix(in srgb, var(--bg-overlay) 60%, transparent);
    border-bottom: 1px solid var(--border-subtle);
  }
  .dot {
    width: 9px; height: 9px;
    border-radius: 50%;
    opacity: 0.55;
  }
  .dot.red   { background: #ff5f57; }
  .dot.amber { background: #febc2e; }
  .dot.green { background: #28c840; }

  .window-body {
    position: absolute;
    inset: 22px 0 0 0;
  }

  /* The anchor button is now tightly sized to the mini-card itself — no
     more giant dashed bounding box swallowing the pill.  An external
     halo (::before) marks the selected anchor without shifting the
     pill's position. */
  .anchor {
    position: absolute;
    width: 64px;
    height: 22px;
    padding: 0;
    background: transparent;
    border: 0;
    border-radius: 8px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: transform var(--anim-dur-fast, 80ms) ease;
  }
  .anchor::before {
    content: '';
    position: absolute;
    inset: -6px;
    border: 1px dashed transparent;
    border-radius: 11px;
    pointer-events: none;
    transition: border-color var(--anim-dur-fast, 80ms) ease,
                background    var(--anim-dur-fast, 80ms) ease;
  }
  .anchor:hover::before {
    border-color: color-mix(in srgb, var(--tone) 40%, transparent);
    background:   color-mix(in srgb, var(--tone) 6%,  transparent);
  }
  .anchor.selected::before {
    border-color: var(--tone);
    background:   color-mix(in srgb, var(--tone) 8%, transparent);
  }

  /* Anchor positioning inside the window-body — measured from the edge
     to keep the mini cards consistently flush. */
  .anchor.pos-top-left      { top:    8px;  left:   10px; }
  .anchor.pos-top-right     { top:    8px;  right:  10px; }
  .anchor.pos-bottom-left   { bottom: 10px; left:   10px; }
  .anchor.pos-bottom-right  { bottom: 10px; right:  10px; }
  .anchor.pos-bottom-center { bottom: 10px; left:   50%; transform: translateX(-50%); }
  /* The centre anchor needs to preserve its translate on hover/selected
     — without this the ::before glow stays put but the button shifts. */
  .anchor.pos-bottom-center:hover,
  .anchor.pos-bottom-center.selected { transform: translateX(-50%); }

  /* The mini-card itself — a tiny mock of the real overlay entry. */
  .anchor-mini {
    display: flex;
    align-items: center;
    gap: 5px;
    width: 100%;
    height: 100%;
    padding: 0 8px;
    background-color: var(--bg-overlay);
    background-image: linear-gradient(180deg, var(--bg-overlay), var(--bg-hover));
    border: 1px solid var(--border);
    border-radius: 7px;
    box-shadow: 0 2px 6px -2px rgba(0, 0, 0, 0.3);
    transition: border-color var(--anim-dur-fast, 80ms) ease,
                box-shadow    var(--anim-dur-fast, 80ms) ease;
  }
  .anchor.selected .anchor-mini {
    border-color: color-mix(in srgb, var(--tone) 65%, var(--border));
    box-shadow: 0 0 14px color-mix(in srgb, var(--tone) 55%, transparent);
  }
  .anchor-stripe {
    width: 2px;
    align-self: stretch;
    background: color-mix(in srgb, var(--tone) 75%, transparent);
    border-radius: 2px;
    opacity: 0.35;
  }
  .anchor.selected .anchor-stripe {
    opacity: 1;
    box-shadow: 0 0 8px color-mix(in srgb, var(--tone) 80%, transparent);
  }
  .anchor-dots {
    display: flex;
    align-items: center;
    gap: 3px;
  }
  .anchor-dots i {
    display: block;
    width: 8px; height: 4px;
    background: var(--text-disabled);
    border-radius: 2px;
    transition: background var(--anim-dur-fast, 80ms) ease;
  }
  .anchor-dots i:nth-child(2) { width: 5px; }
  .anchor-dots i:nth-child(3) { width: 7px; }
  .anchor.selected .anchor-dots i {
    background: color-mix(in srgb, var(--tone) 80%, var(--text-primary));
  }

  /* ── Tone picker ──────────────────────────────────────────────────────── */
  .tone-row {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    justify-content: flex-end;
  }
  .tone-swatch {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 9px 4px 7px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--text-primary);
    font-size: var(--font-size-sm);
    cursor: pointer;
    transition: border-color var(--anim-dur-fast, 80ms) ease,
                background    var(--anim-dur-fast, 80ms) ease;
  }
  .tone-swatch:hover {
    background: color-mix(in srgb, var(--swatch) 12%, var(--bg-overlay));
    border-color: color-mix(in srgb, var(--swatch) 45%, var(--border));
  }
  .tone-swatch.selected {
    background: color-mix(in srgb, var(--swatch) 18%, var(--bg-overlay));
    border-color: var(--swatch);
    color: color-mix(in srgb, var(--swatch) 90%, var(--text-primary));
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--swatch) 30%, transparent);
  }
  .tone-dot {
    width: 11px;
    height: 11px;
    border-radius: 50%;
    background: var(--swatch);
    box-shadow: 0 0 8px color-mix(in srgb, var(--swatch) 55%, transparent);
  }
  .tone-name {
    font-weight: 500;
    letter-spacing: 0.1px;
  }

  /* ── Sliders ──────────────────────────────────────────────────────────── */
  .slider {
    width: 200px;
    accent-color: var(--accent);
  }

  /* ── Toggle row with icon affordance ──────────────────────────────────── */
  .row-icon {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    color: var(--text-muted);
  }

  /* ── Live preview stage ──────────────────────────────────────────────── */
  .preview-stage {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 92px;
    padding: 18px;
    background:
      radial-gradient(circle at 50% 50%,
        color-mix(in srgb, var(--tone) 9%, transparent),
        transparent 65%),
      repeating-linear-gradient(45deg,
        color-mix(in srgb, var(--bg-overlay) 50%, transparent) 0 2px,
        transparent 2px 8px);
    border: 1px dashed var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  /* The preview card mimics the real overlay markup/CSS — kept in sync
     with KeystrokesOverlay.svelte.  When that file changes the visual
     should be replicated here. */
  .preview-stage .ks-card {
    position: relative;
    display: inline-flex;
    align-items: stretch;
    background-color: #2b2d30;
    background-image: linear-gradient(180deg, var(--bg-elevated), var(--bg-base));
    border: 1px solid var(--border);
    border-radius: 14px;
    overflow: hidden;
    color: var(--text-primary);
    opacity: var(--preview-opacity, 1);
    box-shadow:
      0 1px 0 color-mix(in srgb, white 10%, transparent) inset,
      0 0 0 1px color-mix(in srgb, var(--tone) 10%, transparent),
      0 12px 30px -10px rgba(0, 0, 0, 0.55);
  }
  .preview-stage .ks-stripe {
    width: 3px;
    background: linear-gradient(180deg,
      var(--tone),
      color-mix(in srgb, var(--tone) 30%, transparent));
    box-shadow: 0 0 12px color-mix(in srgb, var(--tone) 60%, transparent);
  }
  .preview-stage .ks-body {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 4px;
    padding: 9px 14px 10px 12px;
  }
  .preview-stage .ks-card.compact .ks-body {
    flex-direction: row;
    align-items: center;
    gap: 10px;
    padding: 7px 14px 7px 12px;
  }
  .preview-stage .ks-card.compact.has-action .ks-action::before {
    content: '·';
    display: inline-block;
    margin-right: 8px;
    color: var(--text-disabled);
    font-weight: 700;
  }
  .preview-stage .ks-chord-row {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .preview-stage .ks-chord {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .preview-stage .ks-plus {
    color: var(--text-disabled);
    font-size: 0.78em;
    font-weight: 700;
    user-select: none;
    letter-spacing: 0.5px;
  }
  .preview-stage .ks-key {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 1.85em;
    padding: 0 0.62em;
    height: 1.85em;
    background-color: var(--bg-overlay);
    background-image: linear-gradient(180deg, var(--bg-overlay), var(--bg-hover));
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 7px;
    font-family: var(--font-code);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.3px;
    box-shadow:
      0 1px 0 color-mix(in srgb, white 8%, transparent) inset,
      0 1px 2px rgba(0, 0, 0, 0.18);
  }
  .preview-stage .ks-key.mod {
    background-color: var(--bg-overlay);
    background-image: linear-gradient(180deg,
      color-mix(in srgb, var(--tone) 22%, var(--bg-overlay)),
      color-mix(in srgb, var(--tone) 12%, var(--bg-hover)));
    border-color: color-mix(in srgb, var(--tone) 55%, var(--border));
    color: color-mix(in srgb, var(--tone) 92%, var(--text-primary));
    text-shadow: 0 0 8px color-mix(in srgb, var(--tone) 30%, transparent);
  }
  .preview-stage .ks-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 1px 7px;
    background: color-mix(in srgb, var(--tone) 18%, transparent);
    border: 1px solid color-mix(in srgb, var(--tone) 45%, var(--border));
    border-radius: 999px;
    color: color-mix(in srgb, var(--tone) 92%, var(--text-primary));
    font-family: var(--font-code);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.4px;
  }
  .preview-stage .ks-action {
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 0.15px;
    max-width: 36ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    background: linear-gradient(90deg,
      var(--text-secondary, var(--text-muted)),
      color-mix(in srgb, var(--tone) 35%, var(--text-secondary, var(--text-muted))));
    -webkit-background-clip: text;
            background-clip: text;
    -webkit-text-fill-color: transparent;
            color: transparent;
  }
  /* Size variants — mirror the overlay's. */
  .preview-stage .ks-card.sz-sm .ks-key    { font-size: 10px; min-width: 1.6em; height: 1.6em; padding: 0 0.5em; border-radius: 5px; }
  .preview-stage .ks-card.sz-sm .ks-body   { padding: 7px 11px 8px 10px; gap: 3px; }
  .preview-stage .ks-card.sz-sm .ks-action { font-size: 10px; }
  .preview-stage .ks-card.sz-lg .ks-key    { font-size: 15px; min-width: 2.1em; height: 2.1em; padding: 0 0.75em; border-radius: 9px; border-bottom-width: 3px; }
  .preview-stage .ks-card.sz-lg .ks-body   { padding: 12px 18px 13px 14px; gap: 6px; }
  .preview-stage .ks-card.sz-lg .ks-action { font-size: 13px; }

  /* ── Try it out ──────────────────────────────────────────────────────── */
  .try-card { padding-top: 10px; }
  .try-head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
    color: var(--text-primary);
    font-weight: 600;
    font-size: var(--font-size-sm);
  }
  .try-hint {
    color: var(--text-muted);
    font-weight: 400;
  }
  .try-buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .preset {
    padding: 6px 12px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: var(--font-size-sm);
    cursor: pointer;
    transition: border-color var(--anim-dur-fast, 80ms) ease,
                background    var(--anim-dur-fast, 80ms) ease,
                transform     var(--anim-dur-fast, 80ms) ease;
  }
  .preset:hover {
    background: color-mix(in srgb, var(--accent) 10%, var(--bg-overlay));
    border-color: var(--accent);
    transform: translateY(-1px);
  }
  .preset:active { transform: translateY(0); }
</style>
