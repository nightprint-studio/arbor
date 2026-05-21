// Language → colour dot used by the Repository Browser sidebar and the
// matching legend in DocsPanel. Subset of GitHub's Linguist palette so that
// the sidebar's coloured dots match what users see on github.com.

export const LANGUAGE_COLOURS: Record<string, string> = {
  'TypeScript':   '#3178c6',
  'JavaScript':   '#f1e05a',
  'Python':       '#3572A5',
  'Rust':         '#dea584',
  'Go':           '#00ADD8',
  'Java':         '#b07219',
  'C':            '#555555',
  'C++':          '#f34b7d',
  'C#':           '#178600',
  'Ruby':         '#701516',
  'PHP':          '#4F5D95',
  'Shell':        '#89e051',
  'HTML':         '#e34c26',
  'CSS':          '#563d7c',
  'SCSS':         '#c6538c',
  'Vue':          '#41b883',
  'Svelte':       '#ff3e00',
  'Kotlin':       '#A97BFF',
  'Swift':        '#F05138',
  'Dart':         '#00B4AB',
  'Scala':        '#c22d40',
  'Lua':          '#000080',
  'Elixir':       '#6e4a7e',
  'Haskell':      '#5e5086',
  'Perl':         '#0298c3',
  'R':            '#198CE7',
  'Julia':        '#a270ba',
  'Objective-C':  '#438eff',
  'OCaml':        '#3be133',
  'Clojure':      '#db5855',
  'Erlang':       '#B83998',
  'F#':           '#b845fc',
  'Groovy':       '#4298b8',
  'Solidity':     '#AA6746',
  'TeX':          '#3D6117',
  'Vim Script':   '#199f4b',
  'PowerShell':   '#012456',
  'Dockerfile':   '#384d54',
  'Makefile':     '#427819',
  'Nix':          '#7e7eff',
  'Zig':          '#ec915c',
  'Crystal':      '#000100',
  'Nim':          '#ffc200',
};

const FALLBACK = '#888';

/** Returns the colour for a language, or `transparent` for null. Unknown
 *  languages fall back to a neutral grey so the dot is still rendered. */
export function langColour(lang: string | null | undefined): string {
  if (!lang) return 'transparent';
  return LANGUAGE_COLOURS[lang] ?? FALLBACK;
}

/** Sorted [name, colour] tuples for legend rendering. */
export function languageEntries(): Array<[string, string]> {
  return Object.entries(LANGUAGE_COLOURS).sort((a, b) =>
    a[0].localeCompare(b[0]),
  );
}
