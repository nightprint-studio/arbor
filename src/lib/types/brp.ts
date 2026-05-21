/**
 * Bevy Remote Protocol — frontend types.
 *
 * Phase 1.0: connect/disconnect/call/status only. SSE watch + editing land
 * in later phases (see `project_bevy_brp_client.md` memory). Frontend
 * mirrors the host's serde shapes verbatim — keep field names in sync.
 */

export interface BrpStatus {
  connected: boolean;
  endpoint?: string | null;
  /** Unix seconds since the active session connected. */
  connected_at?: number | null;
}

export interface BrpConnectParams {
  endpoint?: string;
  timeout_ms?: number;
}

/**
 * Error envelope returned by `brp_connect` and `brp_call` when the BRP call
 * itself fails. The frontend can branch on `kind` to differentiate transport
 * failure ("transport"), HTTP-level rejection ("status"), malformed response
 * ("invalid_response"), an actual JSON-RPC error from the game ("rpc"), or
 * the "session-not-connected" case ("not_connected"). "internal" wraps
 * unexpected host-side problems (mutex poisoning, etc.).
 */
export interface BrpCallError {
  kind:
    | 'transport'
    | 'status'
    | 'invalid_response'
    | 'rpc'
    | 'not_connected'
    | 'internal';
  message: string;
  code?: number | null;
  data?: unknown;
}

/** Default BRP endpoint as configured by `bevy_remote::http::DEFAULT_*`. */
export const BRP_DEFAULT_ENDPOINT = 'http://127.0.0.1:15702';

/** BRP 0.18 built-in method names. Mirrors `brp::methods` host-side. */
export const BrpMethod = {
  WorldListComponents:   'world.list_components',
  WorldQuery:            'world.query',
  WorldGetComponents:    'world.get_components',
  WorldSpawnEntity:      'world.spawn_entity',
  WorldDespawnEntities:  'world.despawn_entities',
  WorldInsertComponents: 'world.insert_components',
  WorldRemoveComponents: 'world.remove_components',
  WorldMutateComponents: 'world.mutate_components',
  WorldReparentEntities: 'world.reparent_entities',
  WorldListResources:    'world.list_resources',
  WorldGetResources:     'world.get_resources',
  RegistrySchema:        'registry.schema',
  RpcDiscover:           'rpc.discover',
} as const;
