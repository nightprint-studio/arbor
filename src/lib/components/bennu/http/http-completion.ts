/**
 * Completion for `.http` files: methods on the first line of a request, the common header names on a header line
 * (inserted with their colon), and variables after `{{` — the dynamic ones, the file's own `@name = …`, and the
 * selected environment's.
 */
import type { CompletionContext, CompletionResult, CompletionSource } from '@codemirror/autocomplete';

const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS', 'TRACE'];
const HEADERS = [
  'Accept', 'Accept-Encoding', 'Accept-Language', 'Authorization', 'Cache-Control', 'Content-Type', 'Content-Length', 'Cookie', 'If-Match', 'If-None-Match',
  'Origin', 'Referer', 'SOAPAction', 'User-Agent', 'X-Api-Key', 'X-Correlation-Id', 'X-Requested-With',
];
export const DYNAMIC_VARIABLES = ['$uuid', '$timestamp', '$isoTimestamp', '$randomInt', '$random.integer(1, 100)'];

/** The names an environment contributes; set by the HTTP store when the selection changes. */
let environmentVariables: string[] = [];
export function setEnvironmentVariables(names: string[]) {
  environmentVariables = names;
}

export const httpCompletion: CompletionSource = (ctx: CompletionContext): CompletionResult | null => {
  const line = ctx.state.doc.lineAt(ctx.pos);
  const before = line.text.slice(0, ctx.pos - line.from);

  const variable = /\{\{\s*([\w.$-]*)$/.exec(before);
  if (variable) {
    const own = [...ctx.state.doc.toString().matchAll(/^\s*@([^\s=]+)\s*=/gm)].map((m) => m[1]);
    const names = [...new Set([...own, ...environmentVariables])];
    return {
      from: ctx.pos - variable[1].length,
      options: [
        ...names.map((label) => ({ label, type: 'variable', detail: own.includes(label) ? 'file' : 'environment', apply: `${label}}}` })),
        ...DYNAMIC_VARIABLES.map((label) => ({ label, type: 'constant', detail: 'dynamic', apply: `${label}}}` })),
      ],
      validFor: /^[\w.$()\-, ]*$/,
    };
  }

  const word = /^[A-Za-z-]*$/.exec(before);
  if (!word || line.number === 0) return null;
  let at = line.number - 1;
  while (at >= 1 && ctx.state.doc.line(at).text.trim() === '' && at === line.number - 1) at--;
  const previous = at >= 1 ? ctx.state.doc.line(at).text : '';
  const from = ctx.pos - before.length;
  if (line.number === 1 || /^\s*###/.test(previous)) {
    return { from, options: METHODS.map((label) => ({ label, type: 'keyword', apply: `${label} ` })), validFor: /^[A-Za-z]*$/ };
  }
  if (previous.trim() && /^(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS|TRACE|CONNECT)\s|^[A-Za-z0-9-]+\s*:/.test(previous)) {
    return { from, options: HEADERS.map((label) => ({ label, type: 'property', apply: `${label}: ` })), validFor: /^[A-Za-z-]*$/ };
  }
  return null;
};
