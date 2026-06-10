/**
 * Per-track accent palette. Kept grove-local (not a theme token) so a track's
 * colour is stable regardless of the active Arbor theme — the lanes need to be
 * distinguishable from each other, which a single accent can't do. Values are
 * picked to read well on both light and dark backgrounds.
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
 * Arrangement section marker colours (Logic-style coloured markers in the
 * arrangement track). One distinct hue per section so INTRO / BUILD / FULL /
 * OUTRO read as coloured bands, not plain text.
 */
const SECTION_COLORS = [
  '#4f8ff0', // blue
  '#e0823d', // orange
  '#9d6ad6', // violet
  '#e0556e', // red
  '#3fa7a0', // teal (overflow)
];

export function sectionColor(idx: number): string {
  return SECTION_COLORS[((idx % SECTION_COLORS.length) + SECTION_COLORS.length) % SECTION_COLORS.length];
}
