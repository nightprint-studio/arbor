/**
 * Pasting code into the composer.
 *
 * Decides, from the text alone, whether a paste is code; guesses its language; and says what should go
 * into the box instead — a fenced block with a blank line either side. No DOM and no store, so the
 * rules can be tested: the composer only asks `planPaste` and applies the answer.
 *
 * The test for "is this code" is deliberately conservative. A fenced block that was not wanted costs a
 * keystroke to undo; a paragraph of prose turned into one is an irritation every time, so prose, a
 * list, and anything a single line long are left exactly as they came.
 */
import { isInsideFence } from './assistant-fences';

/** Beyond this a paste is cut: the box and the request are not the place for a whole log file. */
export const MAX_PASTE_CHARS = 200_000;

const EXT_LANGUAGE: Record<string, string> = {
  java: 'java', kt: 'kotlin', kts: 'kotlin', rs: 'rust', ts: 'typescript', tsx: 'typescript', js: 'javascript',
  jsx: 'javascript', mjs: 'javascript', json: 'json', xml: 'xml', xsd: 'xml', xsl: 'xml', html: 'html', htm: 'html',
  jsp: 'html', css: 'css', scss: 'scss', yml: 'yaml', yaml: 'yaml', toml: 'toml', properties: 'properties',
  sql: 'sql', sh: 'bash', bash: 'bash', ps1: 'powershell', bat: 'batch', py: 'python', go: 'go', c: 'c', h: 'c',
  cpp: 'cpp', hpp: 'cpp', cs: 'csharp', lua: 'lua', md: 'markdown', svelte: 'html', gradle: 'java',
};

/** The language a file's extension suggests, or `''`. */
export function languageFromPath(path: string | null | undefined): string {
  const name = path?.split(/[\\/]/).pop() ?? '';
  if (/^pom\.xml$/i.test(name)) return 'xml';
  const dot = name.lastIndexOf('.');
  return dot < 0 ? '' : (EXT_LANGUAGE[name.slice(dot + 1).toLowerCase()] ?? '');
}

const nonEmptyLines = (text: string) => text.split('\n').map((l) => l.replace(/\r$/, '')).filter((l) => l.trim() !== '');

/** A guess from the content alone; `''` when nothing is distinctive enough. */
export function detectLanguage(text: string): string {
  const t = text.trim();
  const lines = nonEmptyLines(t);
  const first = lines[0] ?? '';

  if (/^#!.*\b(ba|z|)sh\b/.test(first) || /^(\$ |sudo |cd |git |mvn |cargo |npm |yarn |docker |export |echo )/.test(first)) return 'bash';

  if (/^[[{]/.test(t)) {
    try { JSON.parse(t); return 'json'; } catch { /* maybe a fragment, below */ }
    if (/^\s*"[\w.-]+"\s*:/m.test(t)) return 'json';
  }

  if (/^<\?xml\b/.test(t)) return 'xml';
  if (/^<!doctype html|^<html\b/i.test(t)) return 'html';
  if (/^<[A-Za-z][\w:.-]*[\s>/]/.test(t) && />\s*$/.test(t)) {
    return /<(div|span|p|body|head|script|a|ul|li|table|form|input|button|h[1-6])\b/i.test(t) ? 'html' : 'xml';
  }

  if (/^\s*(select|insert\s+into|update|delete\s+from|create\s+(table|index|view|unique)|alter\s+table|with)\b/i.test(t) && /\b(from|set|values|table|select|as)\b/i.test(t)) return 'sql';

  if (/^#include\s*[<"]/m.test(t)) return 'cpp';
  if (/\bfn\s+\w+\s*[(<]|\blet\s+mut\b|^\s*impl\b|^\s*use\s+[\w:]+::|^\s*pub\s+(fn|struct|enum|trait)\b|#\[derive/m.test(t)) return 'rust';
  if (/\bfun\s+\w+\s*\(|^\s*data\s+class\b|\bval\s+\w+\s*[:=]/m.test(t)) return 'kotlin';
  if (/^\s*package\s+[\w.]+;|^\s*import\s+[\w.*]+;|@Override\b|System\.out\.|\b(public|private|protected)\s+(static\s+)?(final\s+)?[\w<>[\],? ]+\s+\w+\s*\(|\bpublic\s+(class|interface|enum|record)\b/m.test(t)) return 'java';
  if (/^\s*(def\s+\w+\(.*\)\s*:|from\s+[\w.]+\s+import\b|elif\b)|\bself\./m.test(t)) return 'python';
  if (/:\s*(string|number|boolean|void|any|unknown)\b|^\s*(export\s+)?(interface|type)\s+\w+/m.test(t)) return 'typescript';
  if (/=>|\bfunction\s*\w*\s*\(|\bconsole\.\w+\(|\brequire\(|^\s*(const|let|var)\s+\w+\s*=|^\s*import\s+.+\s+from\s+['"]/m.test(t)) return 'javascript';
  if (/^[\s\S]*[.#\w][\w\s.#,>:-]*\{\s*[\w-]+\s*:[^;{}]+;/m.test(t) && !/\b(function|return|class)\b/.test(t)) return 'css';

  if (/^\[[\w. -]+\]\s*$/m.test(t) && /^\s*[\w.-]+\s*=/m.test(t)) return 'toml';
  if (lines.length >= 2 && lines.every((l) => /^\s*([#;].*|[\w.-]+\s*[=:]\s*.*)$/.test(l)) && lines.filter((l) => /=/.test(l)).length >= 2) return 'properties';
  if (lines.length >= 2 && lines.every((l) => /^\s*(#.*|-(\s.*)?|[\w.-]+:(\s.*)?)$/.test(l)) && lines.filter((l) => /^\s*[\w.-]+:(\s|$)/.test(l)).length >= 2) return 'yaml';
  return '';
}

/** Does this look like source rather than prose? Needs several lines and more than one structural signal. */
export function looksLikeCode(text: string): boolean {
  const lines = nonEmptyLines(text);
  if (lines.length < 2) return false;
  if (detectLanguage(text) === 'json' || /^<\?xml\b/.test(text.trim())) return true;

  const n = lines.length;
  const share = (pred: (l: string) => boolean) => lines.filter(pred).length / n;
  const symbols = (text.match(/[{}[\]();=<>]/g) ?? []).length / Math.max(text.length, 1);

  let score = 0;
  if (share((l) => /[;{}()[\],]\s*$/.test(l)) >= 0.3) score += 2;
  if (share((l) => /^(\t| {2,})\S/.test(l)) >= 0.25) score += 1;
  if (symbols >= 0.04) score += 1;
  if (detectLanguage(text)) score += 2;
  if (lines.some((l) => /^\s*(import|package|public|private|protected|class|interface|def|fn|fun|const|let|var|function|use|#include|return|if\s*\(|for\s*\(|while\s*\()\b/.test(l))) score += 1;

  // Prose: whole sentences, few symbols.
  const sentences = share((l) => /[a-zA-Z]{3,}( [a-zA-Z,'’-]+){5,}[.?!:]\s*$/.test(l));
  if (sentences >= 0.5 && symbols < 0.03) score -= 3;
  return score >= 3;
}

export type PastePlan =
  | { action: 'native' }
  | { action: 'insert'; text: string; language: string; truncated: boolean };

export interface PasteInput {
  /** The clipboard's plain text. */
  pasted: string;
  /** The text before and after the selection that the paste replaces. */
  before: string;
  after: string;
  /** The file open in the editor: the language to fall back on when the content does not say. */
  activePath: string | null;
  enabled: boolean;
}

/** Cut at the last line break under the cap, so the block never ends mid-line. */
function capText(text: string): { text: string; truncated: boolean } {
  if (text.length <= MAX_PASTE_CHARS) return { text, truncated: false };
  const cut = text.lastIndexOf('\n', MAX_PASTE_CHARS);
  return { text: text.slice(0, cut > 0 ? cut : MAX_PASTE_CHARS), truncated: true };
}

/** What a paste should put in the box: the fenced block with the blank lines it needs, or "leave it". */
export function planPaste(input: PasteInput): PastePlan {
  const { pasted, before, after, activePath, enabled } = input;
  if (!enabled) return { action: 'native' };
  if (isInsideFence(before + after, before.length)) return { action: 'native' };

  const { text: capped, truncated } = capText(pasted.replace(/\r\n?/g, '\n'));
  // A line of backticks in the paste would close the fence early; leave such text alone.
  if (/^ {0,3}`{3,}/m.test(capped)) return { action: 'native' };
  // The whole check runs on a bounded prefix: it only needs the shape of the text.
  if (!looksLikeCode(capped.slice(0, 20_000))) return { action: 'native' };

  const code = capped.replace(/^(\s*\n)+/, '').replace(/\s+$/, '');
  const language = detectLanguage(code.slice(0, 20_000)) || languageFromPath(activePath);
  const block = `\`\`\`${language}\n${code}\n\`\`\``;

  const lead = before === '' || before.endsWith('\n\n') ? '' : before.endsWith('\n') ? '\n' : '\n\n';
  const trail = after === '' ? '\n' : after.startsWith('\n\n') ? '' : after.startsWith('\n') ? '\n' : '\n\n';
  return { action: 'insert', text: lead + block + trail, language, truncated };
}
