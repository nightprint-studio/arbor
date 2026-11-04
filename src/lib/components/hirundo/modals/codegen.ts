/**
 * Code for a request in other languages, from the resolved preview — the same value the sender uses, so the
 * snippet does what the window does. Pure string building; no IPC.
 */
import type { Preview } from '$lib/types/hirundo';

export type Lang = 'curl' | 'rust' | 'java' | 'js';

export const LANGS: { id: Lang; label: string; file: string }[] = [
  { id: 'curl', label: 'cURL', file: 'x.sh' },
  { id: 'rust', label: 'Rust · reqwest', file: 'x.rs' },
  { id: 'java', label: 'Java · RestClient', file: 'x.java' },
  { id: 'js', label: 'JavaScript · fetch', file: 'x.js' },
];

const q = (s: string) => JSON.stringify(s);
const sh = (s: string) => `'${s.replace(/'/g, `'\\''`)}'`;

export function generate(lang: Lang, p: Preview): string {
  switch (lang) {
    case 'curl': return curl(p);
    case 'rust': return rust(p);
    case 'java': return java(p);
    case 'js': return js(p);
  }
}

function curl(p: Preview): string {
  const parts = [`curl -X ${p.method} ${sh(p.url)}`];
  for (const [k, v] of p.headers) parts.push(`  -H ${sh(`${k}: ${v}`)}`);
  if (p.body) parts.push(`  --data-raw ${sh(p.body)}`);
  return parts.join(' \\\n');
}

function rust(p: Preview): string {
  const lines = [`let client = reqwest::Client::new();`, `let response = client`, `    .${p.method.toLowerCase()}(${q(p.url)})`];
  for (const [k, v] of p.headers) lines.push(`    .header(${q(k)}, ${q(v)})`);
  if (p.body) lines.push(`    .body(${q(p.body)})`);
  lines.push('    .send()', '    .await?;', 'println!("{}", response.text().await?);');
  return lines.join('\n');
}

function java(p: Preview): string {
  const lines = [`RestClient client = RestClient.create();`, `String body = client.${p.method.toLowerCase()}()`, `    .uri(${q(p.url)})`];
  for (const [k, v] of p.headers) lines.push(`    .header(${q(k)}, ${q(v)})`);
  if (p.body) lines.push(`    .body(${q(p.body)})`);
  lines.push('    .retrieve()', '    .body(String.class);');
  return lines.join('\n');
}

function js(p: Preview): string {
  const init: string[] = [`  method: ${q(p.method)}`];
  if (p.headers.length) init.push(`  headers: {\n${p.headers.map(([k, v]) => `    ${q(k)}: ${q(v)}`).join(',\n')}\n  }`);
  if (p.body) init.push(`  body: ${q(p.body)}`);
  return `const response = await fetch(${q(p.url)}, {\n${init.join(',\n')}\n});\nconsole.log(await response.text());`;
}

