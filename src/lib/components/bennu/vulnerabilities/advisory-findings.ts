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
