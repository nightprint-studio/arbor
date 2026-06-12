/**
 * Per-track accent palette — nemus-local (not a theme token) so a track's colour
 * is stable regardless of the active Arbor theme: the lanes / strips need to be
 * distinguishable from each other, which a single accent can't do. Values read
 * well on both light and dark backgrounds.
 *
 * Used by the arrangement lanes ({@link ArrangementView}), the mixer strips
 * ({@link mixerStore}) and the editor active-hap highlight ({@link nemus-cm}) —
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

/**
 * Stable accent for a **named section** (`section("INTRO", …)`), keyed by name so
 * the same label always reads the same colour across lanes and loops. A distinct
 * warm/structural palette from the lane colours (sections are a backdrop, not a
 * track), hashed by name.
 */
const SECTION_COLORS = [
  '#6c8cd5', // blue
  '#b06ab3', // orchid
  '#d98a3d', // amber
  '#4faf97', // teal
  '#c8607f', // rose
  '#9d8cd8', // violet
];

export function sectionColor(name: string): string {
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) | 0;
  return SECTION_COLORS[((h % SECTION_COLORS.length) + SECTION_COLORS.length) % SECTION_COLORS.length];
}
