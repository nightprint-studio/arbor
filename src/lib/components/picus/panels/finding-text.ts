/**
 * A finding as **text** — what leaves Picus when somebody copies one.
 *
 * It exists because a consistency report is rarely the end of the conversation.
 * It goes into a ticket, a commit message, a chat with whoever wrote the other
 * dialect's half. Retyping a rule id and a path from a panel is exactly the kind
 * of transcription that arrives one character wrong.
 *
 * ## What the shape is for
 *
 * ```
 * CONS001  blocking  ORACLE/AGGIORNAMENTO/4_12__4_13.sql:12
 *   PARAMETRI is not touched by the PostgreSQL scripts
 *   The Oracle initialisation runs 3 statements against PARAMETRI; the
 *   PostgreSQL one runs none, so a fresh PostgreSQL install comes up without it.
 *   also at: POSTGRES/INIZIALIZZAZIONE/02_PARAMETRI.sql:5
 * ```
 *
 * The first line is `path:line`, unindented, because that is the form every
 * editor, terminal and issue tracker already knows how to turn into a jump. The
 * rest is indented so a paste of twenty findings still reads as twenty things.
 *
 * The **consequence is included, and the rule name is not**. A finding's value is
 * the sentence about what goes wrong; "CONS001" is a lookup key, and pasting a
 * catalogue entry into a ticket tells the reader nothing they can act on.
 */

import type { Finding } from '$lib/types/picus';

/** `path:line`, or just the path for a finding that anchors at a whole file. */
export function findingLocation(finding: Finding): string {
  return finding.line ? `${finding.file}:${finding.line}` : finding.file;
}

/** One finding, as it goes onto the clipboard. */
export function findingToText(finding: Finding): string {
  const lines = [
    `${finding.rule}  ${finding.severity}  ${findingLocation(finding)}`,
    `  ${finding.title}`,
    `  ${finding.consequence}`,
  ];
  if (finding.alsoAt) lines.push(`  also at: ${finding.alsoAt}`);
  // A suppressed finding carries the reason it was silenced. Copying it without
  // that would hand somebody a problem that has already been decided about.
  if (finding.suppressedBecause) lines.push(`  suppressed: ${finding.suppressedBecause}`);
  return lines.join('\n');
}

/**
 * A whole report, as text.
 *
 * `heading` names what the list actually is — "12 findings", "3 findings
 * matching 'PARAMETRI'" — because a pasted excerpt that looks like the whole
 * report is worse than one that says it is an excerpt.
 */
export function findingsToText(findings: Finding[], heading: string): string {
  if (!findings.length) return `${heading}\n\n(nothing)`;
  return `${heading}\n\n${findings.map(findingToText).join('\n\n')}`;
}
