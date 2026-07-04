/**
 * Ambient types for the `emmet` core package. The package ships `dist/index.d.ts` but its
 * `package.json` `exports` map declares no `types` condition, so TypeScript (bundler/node16
 * resolution) can't associate the declarations with the import and falls back to `any`. We
 * declare the small surface we actually use (`expandAbbreviation` default + `extract`).
 */
declare module 'emmet' {
  export interface UserConfig {
    type?: 'markup' | 'stylesheet';
    syntax?: string;
    [key: string]: unknown;
  }

  /** Expand an Emmet abbreviation to its markup / stylesheet output. */
  export default function expandAbbreviation(abbr: string, config?: UserConfig): string;

  export interface ExtractedAbbreviation {
    /** The extracted abbreviation text. */
    abbreviation: string;
    /** Location of the abbreviation in the input string. */
    location: number;
    /** Start offset of the matched abbreviation (including any prefix). */
    start: number;
    /** End offset of the extracted abbreviation. */
    end: number;
  }

  export interface ExtractOptions {
    lookAhead: boolean;
    type: 'markup' | 'stylesheet';
    prefix: string;
  }

  /** Extract the Emmet abbreviation ending at `pos` within `line`, or `undefined`. */
  export function extract(
    line: string,
    pos?: number,
    options?: Partial<ExtractOptions>,
  ): ExtractedAbbreviation | undefined;
}
