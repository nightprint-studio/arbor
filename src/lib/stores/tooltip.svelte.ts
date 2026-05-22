/**
 * Tooltip singleton store.
 *
 * Only one tooltip is visible at a time. The `use:tooltip` action publishes
 * here when a trigger requests showing; the `<Tooltip />` host component
 * subscribes and renders.
 */

export type TooltipPlacement = 'top' | 'bottom' | 'left' | 'right' | 'auto';

export interface TooltipOptions {
  /** Primary text. */
  content: string;
  /** Secondary line, dimmer/smaller. */
  description?: string;
  /** Keyboard shortcut hint. Either a "+"-joined string ("Ctrl+K") or an array of keys. */
  shortcut?: string | string[];
  /** Where to place relative to trigger. Default 'auto' (top, flips if no space). */
  placement?: TooltipPlacement;
  /** Open delay in ms. Default 350. */
  delay?: number;
  /** Distance in px from the trigger. Default 8. */
  offset?: number;
  /** Max width in px. Default 320. */
  maxWidth?: number;
  /** Max height in px. Content past this is clipped with a fade. Default 280. */
  maxHeight?: number;
  /** Render `content` as Markdown (sanitised via `renderMarkdown`). Default false. */
  markdown?: boolean;
  /** Skip rendering. */
  disabled?: boolean;
  /** Additional CSS class on the tooltip element (for variant styling). */
  className?: string;
}

export type TooltipInput = string | TooltipOptions;

export interface NormalizedTooltipOptions {
  content: string;
  description?: string;
  shortcut?: string[];
  placement: TooltipPlacement;
  delay: number;
  offset: number;
  maxWidth: number;
  maxHeight: number;
  markdown: boolean;
  disabled: boolean;
  className?: string;
}

export function normalizeOptions(input: TooltipInput): NormalizedTooltipOptions {
  const raw: TooltipOptions = typeof input === 'string' ? { content: input } : input;
  let shortcut: string[] | undefined;
  if (typeof raw.shortcut === 'string') {
    shortcut = raw.shortcut.split('+').map((s) => s.trim()).filter(Boolean);
  } else if (Array.isArray(raw.shortcut)) {
    shortcut = raw.shortcut.filter(Boolean);
  }
  return {
    content: raw.content,
    description: raw.description,
    shortcut,
    placement: raw.placement ?? 'auto',
    delay: raw.delay ?? 350,
    offset: raw.offset ?? 8,
    maxWidth: raw.maxWidth ?? 320,
    maxHeight: raw.maxHeight ?? 280,
    markdown: raw.markdown ?? false,
    disabled: raw.disabled ?? false,
    className: raw.className,
  };
}

export interface ActiveTooltip {
  /** The trigger element — used by the action to update on prop change and by the host to track scroll. */
  trigger: HTMLElement;
  opts: NormalizedTooltipOptions;
  /** Monotonic counter so the host can react even when trigger/opts identity is unchanged. */
  seq: number;
}

const QUICK_REOPEN_MS = 250;

class TooltipState {
  active: ActiveTooltip | null = $state(null);
  private lastHideAt = 0;
  private seq = 0;
  private focusSuppressedUntil = 0;

  /** Returns true if the next show should skip the delay (quick re-hover). */
  shouldSkipDelay(): boolean {
    return performance.now() - this.lastHideAt < QUICK_REOPEN_MS;
  }

  /** Suppress tooltip-on-focus for the next `ms` milliseconds. Callers use
   *  this around programmatic `.focus()` calls (e.g. Modal initial focus)
   *  so the tooltip action doesn't pop a bubble on a focus the user never
   *  asked for. Hover and user-driven focus (Tab, click) are unaffected
   *  once the window elapses. */
  suppressFocusFor(ms: number) {
    this.focusSuppressedUntil = Math.max(this.focusSuppressedUntil, performance.now() + ms);
  }

  /** True while the suppression window is active. */
  isFocusSuppressed(): boolean {
    return performance.now() < this.focusSuppressedUntil;
  }

  show(trigger: HTMLElement, opts: NormalizedTooltipOptions) {
    this.seq += 1;
    this.active = { trigger, opts, seq: this.seq };
  }

  /** Update content while a tooltip is already open for this trigger. */
  update(trigger: HTMLElement, opts: NormalizedTooltipOptions) {
    if (this.active?.trigger === trigger) {
      this.seq += 1;
      this.active = { trigger, opts, seq: this.seq };
    }
  }

  /** Hide unconditionally, or only if the active tooltip belongs to `trigger`. */
  hide(trigger?: HTMLElement) {
    if (trigger && this.active?.trigger !== trigger) return;
    if (this.active) {
      this.active = null;
      this.lastHideAt = performance.now();
    }
  }
}

export const tooltipState = new TooltipState();
