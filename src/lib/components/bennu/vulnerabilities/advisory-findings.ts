// Dependency advisories, as the shared security-findings window understands them.
//
// OSV answers per artifact ("this version has these advisories"); the window shows one row per
// advisory. The mapping lives here so the modal stays a thin shell, and so the one piece of
// vocabulary that differs — OSV says `moderate` where the rest of Arbor says `medium` — is
// normalised in a single place.

import type { ProjectAdvisory, Advisory } from '$lib/ipc/bennu/maven';
import type { SecurityFinding, Severity } from '$lib/types/corvus/security';

export function normalizeSeverity(raw: string): Severity {
  switch (raw.toLowerCase()) {
    case 'critical': return 'critical';
    case 'high': return 'high';
    case 'moderate':
    case 'medium': return 'medium';
    case 'low': return 'low';
    default: return 'unknown';
  }
}

function describe(item: ProjectAdvisory, adv: Advisory): string {
  const where = item.direct
    ? `Declared${item.modules.length ? ` in **${item.modules.join(', ')}**` : ''}.`
    : 'Pulled in transitively by another dependency.';
  const fix = adv.fixed
    ? `Fixed in **${adv.fixed}**.`
    : 'The advisory lists no fixed version.';
  return [adv.summary || 'No summary in the advisory.', '', `\`${item.coord}\` at \`${item.version}\`. ${where} ${fix}`].join('\n');
}

/**
 * The question for the assistant: are we actually exposed to this, and what would fixing it cost?
 *
 * A dependency scanner flags every vulnerable *version*; whether the vulnerable *code* is ever reached is a
 * question about this project, which only reading it can answer. So the assistant is asked to find the
 * affected part of the library, look for its use here, and then weigh the upgrade — and told to change
 * nothing, because the useful result is a verdict the person can act on.
 */
export function assessmentPrompt(f: SecurityFinding): string {
  const ids = f.identifiers.map((i) => i.value).join(', ');
  return [
    `Assess whether this project is really exposed to a known vulnerability in one of its dependencies. Do not change any files: report what you find.`,
    '',
    `**${f.title}**`,
    ids ? `Identifiers: ${ids}` : '',
    f.web_url ? `Advisory: ${f.web_url}` : '',
    f.file_path ? `Declared at ${f.file_path}${f.start_line ? `:${f.start_line}` : ''}` : '',
    '',
    f.description,
    f.solution ? `\nSuggested fix: ${f.solution}` : '',
    '',
    `Do it in this order:`,
    `1. From the advisory, say which part of the library is vulnerable (a class, a method, a feature, a configuration).`,
    `2. Search this project for where that part is used. Is the vulnerable path reachable from our code — and from untrusted input? Cite the files and lines.`,
    `3. Give a verdict: **exposed**, **not exposed**, or **cannot tell**, with the reasoning in a few lines.`,
    `4. Say whether it can be fixed, and what fixing it would cause: the version to move to, what else that version drags in or bumps, breaking changes in the library's API that this code would have to follow, and any module that is affected. If there is no fix, say what would mitigate it.`,
  ].filter((line, i, all) => line !== '' || all[i - 1] !== '').join('\n');
}

/** One finding per (artifact, advisory). */
export function advisoriesToFindings(items: ProjectAdvisory[]): SecurityFinding[] {
  return items.flatMap((item) =>
    item.advisories.map((adv): SecurityFinding => {
      const cve = adv.cve ?? null;
      return {
        id: `${item.coord}@${item.version}#${adv.id}`,
        severity: normalizeSeverity(adv.severity),
        state: 'detected',
        title: `${item.coord} ${item.version} — ${cve ?? adv.id}${adv.summary ? `: ${adv.summary}` : ''}`,
        description: describe(item, adv),
        scanner: 'OSV.dev',
        report_type: 'dependency_scanning',
        file_path: item.direct && item.file ? item.file : null,
        start_line: item.direct && item.line ? item.line : null,
        web_url: `https://osv.dev/vulnerability/${adv.id}`,
        created_at: '',
        age_days: 0,
        identifiers: [
          ...(cve ? [{ kind: 'CVE', value: cve, url: `https://nvd.nist.gov/vuln/detail/${cve}` }] : []),
          { kind: 'GHSA', value: adv.id, url: `https://osv.dev/vulnerability/${adv.id}` },
        ],
        provider: 'github',
        provider_label: 'OSV.dev',
        solution: adv.fixed ? `Upgrade \`${item.coord}\` to **${adv.fixed}** or later.` : null,
      };
    }),
  );
}
