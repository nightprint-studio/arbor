/**
 * Deterministic "audio-like" waveform generation for the arrangement regions.
 * NOT real audio — Step 0 has no engine. Given a seed it produces a stable
 * amplitude envelope (so a region looks the same every render) shaped to feel
 * like the track's character: percussive tracks get spiky transients with gaps,
 * sustained tracks get smooth rolling envelopes.
 */

/** mulberry32 — tiny seeded PRNG (stable, no Math.random so the wave never
 *  flickers between renders). */
function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a |= 0; a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export type WaveKind = 'percussive' | 'sustained' | 'tonal';

/**
 * Build `count` amplitude samples in [0,1].
 *  - percussive: sharp peaks at a pseudo-rhythmic grid, near-silence between.
 *  - sustained:  high, slowly-undulating envelope (pads / drones).
 *  - tonal:      medium body with melodic swells (bass / lead / arp).
 */
export function waveform(count: number, kind: WaveKind, density: number, seed: number): number[] {
  const rand = rng(seed);
  const out: number[] = [];
  // A slow base envelope shared by all kinds (intro/outro fades feel natural).
  const phase = rand() * Math.PI * 2;
  for (let i = 0; i < count; i++) {
    const t = i / Math.max(1, count - 1);
    const slow = 0.5 + 0.5 * Math.sin(phase + t * Math.PI * (1.5 + density * 2));
    let a: number;
    if (kind === 'percussive') {
      // Hits land on a grid every ~4 samples; jitter + decay between them.
      const onGrid = i % 4 === 0;
      const hit = onGrid ? 0.75 + rand() * 0.25 : rand() * 0.18;
      a = hit * (0.6 + 0.4 * density);
    } else if (kind === 'sustained') {
      a = (0.45 + 0.4 * slow) + rand() * 0.1;
    } else {
      // tonal: rolling body with small grain.
      a = 0.3 + 0.45 * slow + (rand() - 0.5) * 0.25;
    }
    out.push(Math.max(0.04, Math.min(1, a)));
  }
  return out;
}

/** Build an SVG path string for a mirrored (top+bottom) waveform filling a
 *  `width × height` box, centred vertically. Samples are 0..1. */
export function wavePath(samples: number[], width: number, height: number): string {
  const n = samples.length;
  if (n === 0) return '';
  const mid = height / 2;
  const step = width / (n - 1 || 1);
  const top: string[] = [];
  const bottom: string[] = [];
  for (let i = 0; i < n; i++) {
    const x = +(i * step).toFixed(2);
    const amp = samples[i] * (mid - 1);
    top.push(`${x},${(mid - amp).toFixed(2)}`);
    bottom.push(`${x},${(mid + amp).toFixed(2)}`);
  }
  bottom.reverse();
  return `M${top.join(' L')} L${bottom.join(' L')} Z`;
}
