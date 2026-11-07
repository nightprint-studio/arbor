/**
 * Patina build — the pure part: what is a Patina file, how a report becomes the diagnostics the Build panel and the editor
 * already show, and the words for the result.
 *
 * The backend (`bennu_patina_build`, `patina_intel.rs`) runs the real compiler in the Patina package and sends a
 * {@link PatinaBuildReport}. Nothing in here touches the backend; the store ({@link bennuRunStore}) calls these.
 */

import type { BuildDiagnostic, PatinaBuildReport } from '$lib/types/bennu';

/** Whether `path` is a Patina source (`.ptn`, any case, either slash). */
export function isPatinaPath(path: string | null | undefined): boolean {
  return !!path && /\.ptn$/i.test(path);
}

/**
 * The report's problems as the build diagnostics the panel lists and the editor marks. The stable code leads the message
 * (`[compile.unsupported] …`), the way the backend does for `bennu_build` on a mod folder, so the two routes read alike.
 */
export function toBuildDiagnostics(report: PatinaBuildReport): BuildDiagnostic[] {
  return report.diagnostics.map((d) => ({
    file: d.file,
    line: d.line,
    col: d.col,
    severity: d.severity,
    message: `[${d.code}] ${d.message}`,
  }));
}

/** How many errors and warnings the report has. */
export function countProblems(report: PatinaBuildReport): { errors: number; warnings: number } {
  let errors = 0;
  let warnings = 0;
  for (const d of report.diagnostics) {
    if (d.severity === 'error') errors++;
    else if (d.severity === 'warning') warnings++;
  }
  return { errors, warnings };
}

/** The one line the log ends with: the verdict, the problems if there are any, and the counts. */
export function patinaHeadline(report: PatinaBuildReport): string {
  const { errors, warnings } = countProblems(report);
  const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;
  if (report.ok) {
    return `Patina build of ${report.mod_id} succeeded${warnings ? ` with ${plural(warnings, 'warning')}` : ''} — ${report.summary}`;
  }
  const what = [plural(errors, 'error'), ...(warnings ? [plural(warnings, 'warning')] : [])].join(', ');
  return `Patina build of ${report.mod_id || 'the mod'} failed — ${what}`;
}
