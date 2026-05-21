/**
 * Shared IDE + project-type constants.
 *
 * Single source of truth for:
 *   • the catalogue of built-in IDEs Arbor knows how to launch (mirror of
 *     `src-tauri/src/git/init.rs::BUILTIN_IDES`);
 *   • the project-type rows surfaced in the UI (Settings → Tools → IDE
 *     Integration → "IDE by Language", and any future per-project picker).
 *
 * Anything that needs to render an IDE label or a project-type row pulls
 * from here so we don't drift between IdeSection, ExternalIntegrations,
 * and any future settings page that shows the same data.
 *
 * Brand icons come from build-time `IconifyIcon` objects in
 * `$lib/utils/brand-icons` — never from Iconify's network API. Render via:
 *
 *     <Icon icon={pt.iconify} width="14" height="14" />
 */
import type { IconifyIcon } from '@iconify/svelte';
import type { ProjectType, IdeEntry } from '$lib/types/git';
import { IDE_ICON, PROJECT_ICON } from '$lib/utils/brand-icons';

/** A built-in IDE entry — mirrors the Rust BUILTIN_IDES table. */
export interface BuiltinIde {
  id:      string;
  name:    string;
  /** Shell-resolvable command that launches the IDE in `PATH`. */
  command: string;
  /** Build-time IconifyIcon for the IDE brand. Render via
   *  `<Icon icon={iconify} style="color: {color}" ... />` so the glyph
   *  shows up in the IDE's brand colour rather than `currentColor`. */
  iconify: IconifyIcon;
  /** Canonical brand colour (hex). Picked to be readable on Arbor's dark
   *  theme — official-but-too-dark values are nudged toward a brighter
   *  variant of the same palette. */
  color:   string;
}

/** Catalogue of IDEs Arbor knows how to launch out of the box.
 *  Order is stable across sections so the dropdown / language matrix
 *  always read the same way. Custom IDEs (`AppConfig.ide.custom_ides`)
 *  are appended at call sites — they're user-defined and live in config.
 *
 *  Brand colours come from the simple-icons palette, with adjustments
 *  for legibility on a dark background:
 *    • JetBrains IDEs use their two-tone scheme's brighter hue (the
 *      lower part of the chevron, not the dark plum top);
 *    • Vim/Neovim follow the established green;
 *    • generic editors (Sublime, VS Code, Cursor) keep their canonical
 *      brand hex. */
export const BUILTIN_IDES: readonly BuiltinIde[] = [
  { id: 'vscode',    name: 'VS Code',        command: 'code',      iconify: IDE_ICON.vscode,    color: '#007ACC' },
  { id: 'cursor',    name: 'Cursor',         command: 'cursor',    iconify: IDE_ICON.cursor,    color: '#A0A0A0' },
  { id: 'zed',       name: 'Zed',            command: 'zed',       iconify: IDE_ICON.zed,       color: '#084CCF' },
  { id: 'intellij',  name: 'IntelliJ IDEA',  command: 'idea',      iconify: IDE_ICON.intellij,  color: '#FF318C' },
  { id: 'webstorm',  name: 'WebStorm',       command: 'webstorm',  iconify: IDE_ICON.webstorm,  color: '#22D88F' },
  { id: 'pycharm',   name: 'PyCharm',        command: 'pycharm',   iconify: IDE_ICON.pycharm,   color: '#21D789' },
  { id: 'rider',     name: 'Rider',          command: 'rider',     iconify: IDE_ICON.rider,     color: '#FCD675' },
  { id: 'clion',     name: 'CLion',          command: 'clion',     iconify: IDE_ICON.clion,     color: '#22D88F' },
  { id: 'goland',    name: 'GoLand',         command: 'goland',    iconify: IDE_ICON.goland,    color: '#0D7CDB' },
  { id: 'rubymine',  name: 'RubyMine',       command: 'rubymine',  iconify: IDE_ICON.rubymine,  color: '#FE2857' },
  { id: 'phpstorm',  name: 'PhpStorm',       command: 'phpstorm',  iconify: IDE_ICON.phpstorm,  color: '#B345F1' },
  { id: 'sublime',   name: 'Sublime Text',   command: 'subl',      iconify: IDE_ICON.sublime,   color: '#FF9800' },
  { id: 'rustrover', name: 'RustRover',      command: 'rustrover', iconify: IDE_ICON.rustrover, color: '#FE2857' },
  { id: 'vim',       name: 'Vim',            command: 'vim',       iconify: IDE_ICON.vim,       color: '#019733' },
  { id: 'neovim',    name: 'Neovim',         command: 'nvim',      iconify: IDE_ICON.neovim,    color: '#57A143' },
];

/** Project-type row used in language-default pickers and per-project UIs.
 *  Subset of `ProjectType` minus `unknown` (which has no language-specific
 *  IDE association). The id matches the Rust `ProjectType` serde name. */
export interface ProjectTypeRow {
  id:      Exclude<ProjectType, 'unknown'>;
  label:   string;
  /** Build-time IconifyIcon — render with `<Icon icon={iconify} ... />`. */
  iconify: IconifyIcon;
  /** Canonical brand colour (hex). Inline-applied via `style="color: …"` so
   *  the icon shows in the language's brand hue rather than the surrounding
   *  text colour. Tuned for readability on a dark background — official
   *  hexes that disappear into the chrome (e.g. Gradle's `#02303A`) are
   *  nudged to a brighter variant of the same palette. */
  color:   string;
}

/** Human-facing project-type catalogue. Glyphs come from the `simple-icons`
 *  collection (clean monochrome marks that scale well at 14 px), tinted
 *  per-row with the language's brand colour so the section reads at a
 *  glance instead of a wall of theme-coloured shapes. */
export const PROJECT_TYPES: readonly ProjectTypeRow[] = [
  { id: 'rust',        label: 'Rust',          iconify: PROJECT_ICON.rust,        color: '#CE412B' },
  { id: 'node_js',     label: 'Node.js',       iconify: PROJECT_ICON.node_js,     color: '#5FA04E' },
  { id: 'java_maven',  label: 'Java (Maven)',  iconify: PROJECT_ICON.java_maven,  color: '#C71A36' },
  // Gradle's official `#02303A` is unreadable on dark — use the brighter
  // teal accent from their docs / hero gradient instead.
  { id: 'java_gradle', label: 'Java (Gradle)', iconify: PROJECT_ICON.java_gradle, color: '#1BA39C' },
  { id: 'go',          label: 'Go',            iconify: PROJECT_ICON.go,          color: '#00ADD8' },
  { id: 'python',      label: 'Python',        iconify: PROJECT_ICON.python,      color: '#3776AB' },
  { id: 'dot_net',     label: '.NET',          iconify: PROJECT_ICON.dot_net,     color: '#7B5BD2' },
  { id: 'cpp',         label: 'C / C++',       iconify: PROJECT_ICON.cpp,         color: '#659AD2' },
  { id: 'ruby',        label: 'Ruby',          iconify: PROJECT_ICON.ruby,        color: '#CC342D' },
  { id: 'php',         label: 'PHP',           iconify: PROJECT_ICON.php,         color: '#777BB4' },
];

/** Resolve a user-friendly label for an IDE id, looking through both the
 *  built-in catalogue and a caller-supplied custom-IDE list (typically
 *  `AppConfig.ide.custom_ides`). Returns the raw id when nothing matches —
 *  callers can render that as-is rather than throwing. */
export function findIdeLabel(
  id:         string,
  customIdes: ReadonlyArray<IdeEntry> = [],
): string {
  if (!id) return '';
  const builtin = BUILTIN_IDES.find(b => b.id === id);
  if (builtin) return builtin.name;
  const custom = customIdes.find(c => c.id === id);
  return custom ? `${custom.name} (custom)` : id;
}

/** Build a flat `{id, name}` list combining built-in + custom IDEs in the
 *  canonical order. Used by every dropdown that lets the user pick an IDE
 *  (default, per-language, per-project). */
export function listAllIdes(
  customIdes: ReadonlyArray<IdeEntry> = [],
): { id: string; name: string }[] {
  return [
    ...BUILTIN_IDES.map(b => ({ id: b.id, name: b.name })),
    ...customIdes.map(c => ({ id: c.id, name: `${c.name} (custom)` })),
  ];
}
