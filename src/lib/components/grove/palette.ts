/**
 * Per-track accent palette — grove-local (not a theme token) so a track's colour
 * is stable regardless of the active Arbor theme: the lanes / strips need to be
 * distinguishable from each other, which a single accent can't do. Values read
 * well on both light and dark backgrounds.
 *
 * Used by the arrangement lanes ({@link ArrangementView}), the mixer strips
 * ({@link mixerStore}) and the editor active-hap highlight ({@link grove-cm}) —
 * one source of truth keyed by the BE-stable strip index.
 */
const LANE_COLORS = [
  '#5b9bd5', // bass — blue
  '#9d7cd8', // pad — violet
  '#e0823d', // drums — amber
  '#54b399', // arp — teal
  '#e15c7f', // lead — rose
  '#d4a843', // gold
  '#6cabdd', // sky
  '#b06ab3', // orchid
];

export function laneColor(idx: number): string {
  return LANE_COLORS[((idx % LANE_COLORS.length) + LANE_COLORS.length) % LANE_COLORS.length];
}
