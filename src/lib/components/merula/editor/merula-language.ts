/**
 * The `.merula` language as a descriptor for the shared code editor (`shared/ui/code-editor`):
 * which grammar parses it, how a leaf is coloured, what completes and what shows on hover, and
 * the merula-only layers (active haps, occurrence marks, advisory underlines) in the `editing`
 * slot. The editor itself — history, gutters, search, lint, folding, go-to — is not merula's.
 *
 * A descriptor per editor instance rather than a module constant, because the layers report to
 * the host (the symbol under the caret, the selection, an instrument preview) through callbacks.
 */

import type { LanguageDescriptor } from '$lib/components/shared/ui/code-editor/types';
import { classifyToken, createMerulaParser } from './merula-lang';
import { createMerulaFeatureExtensions, type MerulaFeatureOptions } from './merula-cm';
import { merulaCompletion, merulaHoverSource, type MerulaIntelSource } from './merula-intel';
import { merulaLanguageEditing } from './merula-ergonomics';

export interface MerulaLanguageOptions extends MerulaFeatureOptions {
  /** The DSL catalogue for autocomplete + hover docs. Omitted → highlight and lint only (a read-only sample). */
  intel?: MerulaIntelSource;
}

export function createMerulaLanguage(opts: MerulaLanguageOptions = {}): LanguageDescriptor {
  const { intel, ...features } = opts;
  return {
    id: 'merula',
    createParser: createMerulaParser,
    // The token classes are merula's own (`note`, `chord`, `sound`, …): the shared highlighter emits
    // `cm-tok-<class>` for any name, and `merulaTheme` colours them.
    classify: (node, isNamed, field, parentType) => classifyToken(node.type, isNamed, field, parentType),
    commentTokens: { line: '//', block: { open: '/*', close: '*/' } },
    // Folds come from the bracket scanner in `editing`; this only installs the gutter that shows them.
    serverFold: true,
    editing: [merulaLanguageEditing(), createMerulaFeatureExtensions(features)],
    intel: intel
      ? { completion: merulaCompletion(intel), hover: merulaHoverSource(intel) }
      : undefined,
  };
}
