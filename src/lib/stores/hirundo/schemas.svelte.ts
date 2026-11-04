/**
 * Hirundo › the shapes of the DTOs of detected endpoints — asked of Bennu once per endpoint and kept, so the
 * editors can complete `body.…` in a test or a key in a JSON body without a round trip per keystroke.
 */
import { endpointSchema } from '$lib/ipc/hirundo';
import type { EndpointSchema } from '$lib/types/hirundo';

function createHirundoSchemas() {
  let cache = $state<Record<string, EndpointSchema | null>>({});
  const asked = new Set<string>();

  const key = (root: string, id: string) => `${root}::${id}`;

  /** Ask for the schema of an endpoint (once). The answer shows up in {@link of}. */
  function load(root: string, id: string) {
    const k = key(root, id);
    if (asked.has(k)) return;
    asked.add(k);
    void endpointSchema(root, id).then((s) => { cache[k] = s; }).catch(() => { cache[k] = null; });
  }

  return {
    load,
    of: (root: string, id: string): EndpointSchema | null => cache[key(root, id)] ?? null,
    /** Ask and answer in one: for the moment a request is made from an endpoint. */
    async get(root: string, id: string): Promise<EndpointSchema | null> {
      const k = key(root, id);
      if (!(k in cache)) {
        asked.add(k);
        cache[k] = await endpointSchema(root, id).catch(() => null);
      }
      return cache[k];
    },
  };
}

export const hirundoSchemas = createHirundoSchemas();
