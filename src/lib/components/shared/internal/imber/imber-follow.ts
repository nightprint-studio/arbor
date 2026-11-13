/**
 * Following the end of an Imber thread, and coming back to where each conversation was left.
 *
 * The thread follows new text while the reader is at its end, and stops the moment they scroll up — that is
 * the reader saying "not now". Checking the position after the content has grown cannot tell those apart (a
 * long chunk lands, and the end is suddenly far away), so the follower keeps the reader's intent instead: it
 * is "stuck" to the end until the reader moves up, and stuck again once they come back down. The content is
 * watched for size, not for messages, so anything that grows it — streamed text, a card, a picture that
 * loaded, a highlighted code block — is followed.
 *
 * Each conversation's place is remembered for the session (in memory: a scroll position is not a setting):
 * reopening the panel lands where the reader was, or at the end if that is where they were — and stays there
 * while the thread finishes laying out, which is what used to leave it a little above the end.
 */

import { isNearBottom } from './imber-thread';

interface Place {
  top: number;
  atEnd: boolean;
}

const places = new Map<string, Place>();

/** How long after a conversation opens its remembered place is held while the content settles. */
const SETTLE_MS = 2000;
/** How long after a wheel turn, a key or a touch a scroll still counts as the reader's. */
const USER_SCROLL_MS = 400;

export class ThreadFollower {
  private stuck = true;
  private key = '';
  private lastTop = 0;
  private lastHeight = 0;
  /** The top our own last scroll set: its scroll event is not the reader's. */
  private ownTop: number | null = null;
  /** The remembered place being held while the thread lays out, until the reader moves. */
  private restore: { top: number; until: number } | null = null;
  /** Until when a scroll counts as the reader's own: set by the wheel, the keys, a touch or a drag of the bar. */
  private userUntil = 0;
  /** A pointer is down on the scroller — dragging the scroll bar, for as long as it is held. */
  private dragging = false;
  private readonly observer: ResizeObserver;
  /** Changes that do not resize anything the size observer watches — a line swapped for another, a summary that
   *  changed its words — and that land after the last jump to the end. */
  private readonly mutations: MutationObserver;
  /** A second look at the end, one frame after a jump: what laid out after it is caught there. */
  private frame: number | null = null;

  constructor(
    private readonly el: HTMLElement,
    content: HTMLElement,
  ) {
    el.addEventListener('scroll', this.onScroll, { passive: true });
    el.addEventListener('wheel', this.onWheel, { passive: true });
    el.addEventListener('keydown', this.onUser, { passive: true });
    el.addEventListener('touchstart', this.onUser, { passive: true });
    el.addEventListener('pointerdown', this.onPointerDown, { passive: true });
    window.addEventListener('pointerup', this.onPointerUp, { passive: true });
    this.observer = new ResizeObserver(() => this.settle());
    this.observer.observe(content);
    this.observer.observe(el);
    this.mutations = new MutationObserver(() => this.settle());
    this.mutations.observe(content, { childList: true, subtree: true, characterData: true });
  }

  /** Show the conversation `key`: at its remembered place, or at the end. */
  open(key: string) {
    if (key === this.key) return;
    this.remember();
    this.key = key;
    const place = places.get(key);
    if (!place || place.atEnd) {
      this.restore = null;
      this.stuck = true;
      this.toEnd();
    } else {
      this.stuck = false;
      this.restore = { top: place.top, until: Date.now() + SETTLE_MS };
      this.scrollTo(place.top);
    }
  }

  /** Something was added: follow it if the reader is at the end. */
  grew() {
    this.settle();
  }

  /** Jump to the end and follow again — the reader asked for it (sent a message). */
  stick() {
    this.restore = null;
    this.stuck = true;
    this.toEnd();
  }

  destroy() {
    this.remember();
    this.observer.disconnect();
    this.mutations.disconnect();
    if (this.frame !== null) cancelAnimationFrame(this.frame);
    this.el.removeEventListener('scroll', this.onScroll);
    this.el.removeEventListener('wheel', this.onWheel);
    this.el.removeEventListener('keydown', this.onUser);
    this.el.removeEventListener('touchstart', this.onUser);
    this.el.removeEventListener('pointerdown', this.onPointerDown);
    window.removeEventListener('pointerup', this.onPointerUp);
  }

  private settle() {
    if (this.stuck) {
      this.toEnd();
    } else if (this.restore && Date.now() < this.restore.until) {
      this.scrollTo(this.restore.top);
    } else {
      this.restore = null;
    }
    this.lastHeight = this.el.scrollHeight;
  }

  private remember() {
    if (this.key) places.set(this.key, { top: this.el.scrollTop, atEnd: this.stuck });
  }

  private toEnd() {
    this.scrollTo(this.el.scrollHeight);
    // Whatever finishes laying out after this — an animation's last frame, a block drawn a tick later — would leave
    // the view a few pixels above the end with nothing to bring it down. One frame later, look again.
    if (this.frame === null) {
      this.frame = requestAnimationFrame(() => {
        this.frame = null;
        const { scrollTop, scrollHeight, clientHeight } = this.el;
        if (this.stuck && !isNearBottom(scrollHeight, scrollTop, clientHeight, 2)) this.scrollTo(scrollHeight);
      });
    }
  }

  private scrollTo(top: number) {
    this.el.scrollTo({ top, behavior: 'instant' });
    this.ownTop = this.el.scrollTop;
    this.lastTop = this.el.scrollTop;
  }

  /** The wheel moving up is the clearest "stop following" there is — before any scroll event says so. */
  private onWheel = (e: WheelEvent) => {
    this.userUntil = Date.now() + USER_SCROLL_MS;
    if (e.deltaY < 0) {
      this.stuck = false;
      this.restore = null;
    }
  };

  private onUser = () => {
    this.userUntil = Date.now() + USER_SCROLL_MS;
  };

  private onPointerDown = () => {
    this.dragging = true;
  };

  private onPointerUp = () => {
    if (this.dragging) this.userUntil = Date.now() + USER_SCROLL_MS;
    this.dragging = false;
  };

  private onScroll = () => {
    const { scrollTop, scrollHeight, clientHeight } = this.el;
    const own = this.ownTop !== null && Math.abs(scrollTop - this.ownTop) < 2;
    this.ownTop = null;
    this.lastHeight = scrollHeight;
    // Only the reader decides to stop following, and the reader acts through the wheel, the keys, a touch or the
    // scroll bar. Anything else that moves the view — a block above the end shrinking, the browser keeping an
    // anchor in place, a fold re-rendering — is layout: while following, the view goes back to the end.
    const byReader = this.dragging || Date.now() < this.userUntil;
    if (!own && byReader) {
      if (scrollTop < this.lastTop - 1) {
        this.stuck = false;
        this.restore = null;
      } else if (isNearBottom(scrollHeight, scrollTop, clientHeight, 24)) {
        this.stuck = true;
      }
    } else if (!own && this.stuck && !isNearBottom(scrollHeight, scrollTop, clientHeight, 2)) {
      this.toEnd();
      return;
    }
    this.lastTop = scrollTop;
    this.remember();
  };
}
