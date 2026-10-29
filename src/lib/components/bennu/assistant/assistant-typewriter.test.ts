import { describe, expect, it } from 'vitest';
import { nextLength } from './assistant-typewriter';

describe('the pace of the assistant\'s text', () => {
  it('shows at least a character a frame while something is waiting', () => {
    expect(nextLength(0, 5, 16)).toBeGreaterThanOrEqual(1);
    expect(nextLength(10, 11, 1)).toBe(11);
  });

  it('never goes past what has been received, nor back', () => {
    expect(nextLength(8, 8, 16)).toBe(8);
    expect(nextLength(12, 8, 16)).toBe(8);
    expect(nextLength(0, 3, 64)).toBe(3);
  });

  it('speeds up on a long backlog so the display does not fall behind', () => {
    const slow = nextLength(0, 20, 16);
    const fast = nextLength(0, 4000, 16);
    expect(fast).toBeGreaterThan(slow);
    // 4000 characters are worked off in well under two seconds at 60 frames a second.
    let shown = 0, frames = 0;
    while (shown < 4000 && frames < 1000) { shown = nextLength(shown, 4000, 16); frames++; }
    expect(frames).toBeLessThan(120);
  });

  it('does not let one stalled frame jump the text', () => {
    expect(nextLength(0, 100_000, 5_000)).toBe(nextLength(0, 100_000, 64));
  });
});
