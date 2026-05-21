/**
 * YAML ↔ .properties converter — Phase 5.b extension (2026-05-16).
 *
 * Calls the host-side codec in `studio::format::properties_codec`.
 * Lives outside `studio-format.ts` because it isn't a per-format
 * backend method — it's a cross-format bridge. Phase 6 (.properties
 * Studio) will reuse the same commands when it lands.
 */

import { invoke } from '@tauri-apps/api/core';

/** Result of `yaml → .properties`. */
export interface YamlToPropertiesOutput {
  properties_text: string;
  /** Lossy-transformation summary (anchor expansion, comment count, …). */
  warnings:        string[];
}

/** Result of `.properties → yaml`. */
export interface PropertiesToYamlOutput {
  yaml_text: string;
  warnings:  string[];
}

/** Convert a YAML document to `.properties` text. Rejects multi-doc
 *  YAML streams with a structured error. */
export const yamlToProperties = (text: string): Promise<YamlToPropertiesOutput> =>
  invoke<YamlToPropertiesOutput>('studio_yaml_to_properties', { text });

/** Convert `.properties` text to a YAML document. `strings_only=true`
 *  disables best-effort scalar type inference (every value stays a
 *  string in the output YAML). */
export const propertiesToYaml = (
  text: string,
  opts: { stringsOnly?: boolean } = {},
): Promise<PropertiesToYamlOutput> =>
  invoke<PropertiesToYamlOutput>('studio_properties_to_yaml', {
    text,
    stringsOnly: opts.stringsOnly ?? false,
  });
