/**
 * What a kind of template looks like in the interface — its icon, and the colour a language wears.
 *
 * Here rather than in each consumer because the settings tree, the overview and the list of a kind
 * all draw the same six things: an icon that means a different kind in two places is a worse
 * mistake than no icon at all.
 */

import { Braces, FileCog, FilePlus2, FlaskConical, Type } from 'lucide-svelte';
import { JINJA_INNER_BY_EXTENSION, JINJA_LANGUAGE_LABELS } from '$lib/utils/jinja-words';
import type { TemplateKindId } from '$lib/ipc/bennu/templates';
import type { IconComponent } from '$lib/types/icon';

/** One icon per kind, in the vocabulary the rest of Bennu uses: a file, a class, a configuration, a
 *  test, a word you type. */
export const TEMPLATE_KIND_ICONS: Record<TemplateKindId, IconComponent> = {
  'new-file': FilePlus2,
  class: Braces,
  'config-properties': FileCog,
  'config-class': FileCog,
  'validation-tests': FlaskConical,
  live: Type,
};

/**
 * The tone a language's pill wears.
 *
 * Three families and a neutral, not one colour per language: sixteen tints is a paint chart nobody
 * reads, while "this is the JVM one / the systems one / the data one" is a distinction the eye can
 * make down a list without being told.
 */
export type LanguageTone = 'warning' | 'danger' | 'info' | 'success' | 'neutral';

const TONE_OF_LANGUAGE: Record<string, LanguageTone> = {
  java: 'warning', kotlin: 'warning',
  rust: 'danger', go: 'danger',
  python: 'info', javascript: 'info', lua: 'info', shell: 'info',
  xml: 'success', html: 'success', yaml: 'success', properties: 'success', toml: 'success',
  json: 'success', sql: 'success', css: 'success',
};

/** What a template writes, as a person reads it: `java` → `Java`, an empty extension → plain text. */
export function languageOf(extension: string): { label: string; tone: LanguageTone } {
  const language = JINJA_INNER_BY_EXTENSION[extension.toLowerCase()] ?? '';
  if (!language) {
    return { label: extension ? extension.toLowerCase() : 'text', tone: 'neutral' };
  }
  return {
    label: JINJA_LANGUAGE_LABELS[language] ?? language,
    tone: TONE_OF_LANGUAGE[language] ?? 'neutral',
  };
}
