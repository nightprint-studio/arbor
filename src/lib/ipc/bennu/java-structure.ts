/**
 * A Java buffer's declarations, read from the parse tree.
 *
 * Wire: `bennu_java_structure` — `{ source }`, wrapped under `args` like the rest of `ipc/bennu`.
 * The entries come back in source order, each with how many types enclose it, which is all the
 * Structure panel and the File Structure popup need to build their views.
 */

import { bennu } from '../rpc';

export interface JavaStructureEntry {
  /** `class` · `interface` · `enum` · `record` · `annotation`, or `method` · `field`. */
  kind: string;
  name: string;
  /** A method's return type, a field's type. */
  detail?: string;
  /** 1-based line. */
  line: number;
  visibility: 'public' | 'protected' | 'private' | 'package';
  /** A method carrying `@Override`. */
  overrides?: boolean;
  /** How many types enclose it: a top-level type is 0, its members 1. */
  depth: number;
  isType: boolean;
}

export function javaStructureEntries(source: string): Promise<JavaStructureEntry[]> {
  return bennu('bennu_java_structure', { args: { source } });
}
