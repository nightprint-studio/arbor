/**
 * Bennu inspections IPC — which checks a project reports, and how loudly.
 *
 * The `[inspections]` section of `<repo>/.arbor/bennu/config.toml`: one severity per check code,
 * absent meaning the check's own default. The catalog is what a settings screen renders from — every
 * check kind the build knows, with its default and whatever this project said instead — so a check
 * added in Rust is configurable the day it lands, with no change here.
 *
 * Routes through the generic `bennu(...)` bridge.
 */

import { bennu } from '../rpc';

/**
 * How loudly a check reports.
 *
 * `weak` is its own level and not a quieter warning: it is drawn faintly and grouped apart, for
 * something that is true but is not a defect — a name breaking a house style, say.
 */
export type CheckSeverity = 'error' | 'warning' | 'weak' | 'off';

/** Every severity, in the order a dropdown should list them — loudest first, silence last. */
export const CHECK_SEVERITIES: CheckSeverity[] = ['error', 'warning', 'weak', 'off'];

/** One check kind, as the settings screen needs it. */
export interface CheckInfo {
  /** The stable slug — the key the config is written under, and the one a `@SuppressWarnings`
   *  names. */
  code: string;
  /** A readable name, derived from the code so the two cannot drift. */
  title: string;
  /** What it reports at when the project says nothing. */
  defaultSeverity: CheckSeverity;
  /** The level this project configured, when it differs from the default. Absent otherwise. */
  configured?: CheckSeverity;
}

/** The `[inspections]` section: a level per check code. */
export interface InspectionConfig {
  /** `code` → severity. A code absent here reports at its own default. */
  severity: Record<string, CheckSeverity>;
}

/** Every check kind, with what it reports at in this project. Wire: `bennu_inspection_catalog`. */
export function inspectionCatalog(root: string): Promise<CheckInfo[]> {
  return bennu('bennu_inspection_catalog', { args: { root } });
}

/** Read `[inspections]`. Wire: `bennu_get_inspection_config`. */
export function getInspectionConfig(root: string): Promise<InspectionConfig> {
  return bennu('bennu_get_inspection_config', { args: { root } });
}

/** Persist `[inspections]`, leaving every other section intact. Wire: `bennu_set_inspection_config`. */
export function setInspectionConfig(root: string, config: InspectionConfig): Promise<void> {
  return bennu('bennu_set_inspection_config', { args: { root, config } });
}
