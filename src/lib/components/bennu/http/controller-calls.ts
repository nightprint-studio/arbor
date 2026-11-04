/**
 * "Run this call" from a Spring controller: an endpoint the index found becomes a `.http` request — method, URL
 * on `{{baseUrl}}`, its query parameters, a JSON body when the handler takes one — which Hirundo's engine runs.
 * Pure text building; the editor and the HTTP store do the rest.
 */
import { bennu } from '$lib/ipc/rpc';
import type { DetectedEndpoint } from '$lib/types/hirundo';

export const endpointsInFile = (root: string, file: string) => bennu<DetectedEndpoint[]>('endpoints_in_file', { root, file });

/** `/orders/{id}` → `/orders/{{id}}`. */
const template = (path: string) => path.replace(/\{([^}/]+)\}/g, '{{$1}}');

/**
 * The request as `.http` text. `baseUrl` is the address to start from when no environment gives one: it goes in
 * as an `@baseUrl = …` line the user can edit or delete.
 */
export function endpointToHttpText(e: DetectedEndpoint, opts: { baseUrl?: string } = {}): string {
  const lines: string[] = [];
  if (opts.baseUrl) lines.push(`@baseUrl = ${opts.baseUrl}`, '');
  lines.push(`### ${e.controller}.${e.handler}`);
  const query = e.params.filter((p) => /param|query/i.test(p.binding) && p.required).map((p) => `${p.name}=`);
  lines.push(`${(e.methods[0] ?? 'GET').toUpperCase()} {{baseUrl}}${template(e.path.startsWith('/') ? e.path : `/${e.path}`)}${query.length ? `?${query.join('&')}` : ''}`);
  if (e.produces) lines.push(`Accept: ${e.produces}`);
  const hasBody = e.params.some((p) => /body/i.test(p.binding));
  if (hasBody) lines.push('Content-Type: application/json', '', '{', '  ', '}');
  return lines.join('\n') + '\n';
}

/** The line (1-based) of the request in the text `endpointToHttpText` made. */
export function requestLineOf(text: string): number {
  const lines = text.split('\n');
  return lines.findIndex((l) => /^(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)\s/.test(l)) + 1;
}
