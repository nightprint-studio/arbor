/**
 * Shared types for the three <StudioModal> footer slot components
 * (StudioFooterStatus / StudioFooterCenter / StudioFooterRight).
 *
 * The `StudioFooterDoc` shape is a plain snapshot — the wrapper passes
 * primitives derived from its own store, so the footer components don't
 * pull on a specific store API. This keeps the footer reusable across
 * single-doc stores (JSON / TOML / YAML / Properties) and the
 * multi-tab workspace store (RON).
 */
import type { EncodingInfo } from '$lib/ipc/studio-format';

export interface StudioFooterDoc {
  parseError: string | null;
  dirty:      boolean;
  sourcePath: string | null;
  encoding:   EncodingInfo | null;
  canUndo:    boolean;
  canRedo:    boolean;
  /** Used as the gate for the encoding pill: rendered only when the
   *  document is currently open (i.e. has a docId). */
  docId:      string | null;
}

export interface IndentChoice {
  id:    string;
  label: string;
  unit:  string;
}

/** Default indent picker — 2sp / 4sp / Tab. RON + JSON inject an extra
 *  `8sp` entry via the `indentOptions` prop. */
export const DEFAULT_INDENT_OPTIONS: IndentChoice[] = [
  { id: 'sp2', label: '2 spaces', unit: '  ' },
  { id: 'sp4', label: '4 spaces', unit: '    ' },
  { id: 'tab', label: 'Tab',      unit: '\t' },
];

/** Default indent picker with the 8-space option used by RON / JSON. */
export const INDENT_OPTIONS_WITH_8: IndentChoice[] = [
  { id: 'sp2', label: '2 spaces', unit: '  ' },
  { id: 'sp4', label: '4 spaces', unit: '    ' },
  { id: 'sp8', label: '8 spaces', unit: '        ' },
  { id: 'tab', label: 'Tab',      unit: '\t' },
];
