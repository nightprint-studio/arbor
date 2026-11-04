/** Mock answers for the "hirundo" area — see fixtures/index.ts. Keys: Tauri command, or `program.method` for rpc. */
import type { FixtureHandler } from './index';

const PROJECT = '/Users/dev/gestionale';

const kv = (key: string, value: string, description = '') => ({ key, value, description, enabled: true });

const request = (name: string, method: string, url: string, extra: Record<string, unknown> = {}) => ({
  name, order: 0, method, url, params: [], headers: [], auth: { type: 'inherit' },
  body: { kind: 'none', text: '', form: [] }, settings: { timeout_ms: 30000, follow_redirects: true, verify_tls: true },
  docs: '', tests: '', ...extra,
});

const LIST = request('Lista bandi', 'GET', '{{baseUrl}}/api/bandi', {
  params: [kv('stato', 'APERTO', 'enum StatoBando'), kv('page', '0', 'da @RequestParam')],
  headers: [kv('Accept', 'application/json')],
  tests: 'status == 200\ntime < 500ms\nbody.content[*].stato == "APERTO"\n--- lua\nhirundo.expect(#hirundo.response.json().content > 0, "almeno un bando")',
});

const BODY = JSON.stringify({ content: [{ id: 42, titolo: 'Bando mobilità 2026', stato: 'APERTO' }, { id: 43, titolo: 'Bando asili nido', stato: 'APERTO' }], totalElements: 2 });

const RESPONSE = {
  status: 200, status_text: 'OK', http_version: 'HTTP/1.1',
  headers: [['content-type', 'application/json'], ['cache-control', 'no-store']],
  body: BODY, body_base64: null, size_bytes: BODY.length, truncated: false, time_ms: 184, final_url: 'https://stg.example.it/api/bandi?stato=APERTO&page=0',
};

const TYPES = [
  { id: 'local', name: 'Local', color: '#3fb950', confirm_mutating: false, vars: [] },
  { id: 'dev', name: 'Dev', color: '#2f81f7', confirm_mutating: false, vars: [] },
  { id: 'staging', name: 'Staging', color: '#d29922', confirm_mutating: false, vars: [kv('timeout', '5000')] },
  { id: 'prod', name: 'Prod', color: '#f85149', confirm_mutating: true, vars: [] },
];

const CONFIG = {
  last_project: PROJECT,
  active_workspace: 'ws1',
  recent_projects: [PROJECT, '/Users/dev/anagrafe', '/Users/dev/protocollo'],
  detected_roots: [],
  workspaces: [
    { id: 'ws1', name: 'Gestionale', color_idx: 0, projects: [PROJECT, '/Users/dev/anagrafe'] },
    { id: 'ws2', name: 'Protocollo', color_idx: 3, projects: ['/Users/dev/protocollo'] },
  ],
};

const ENDPOINTS = [
  { id: 'BandoController#list', methods: ['GET'], path: '/api/bandi', controller: 'BandoController', handler: 'list', return_type: 'Page<BandoDto>', produces: 'application/json', params: [{ name: 'stato', type_text: 'StatoBando', binding: 'RequestParam', required: false }, { name: 'page', type_text: 'int', binding: 'RequestParam', required: false }], file: '/Users/dev/gestionale/src/main/java/it/example/BandoController.java', line: 24 },
  { id: 'BandoController#create', methods: ['POST'], path: '/api/bandi', controller: 'BandoController', handler: 'create', return_type: 'BandoDto', produces: 'application/json', params: [{ name: 'body', type_text: 'BandoDto', binding: 'RequestBody', required: true }], file: '/Users/dev/gestionale/src/main/java/it/example/BandoController.java' },
  { id: 'BandoController#get', methods: ['GET'], path: '/api/bandi/{id}', controller: 'BandoController', handler: 'get', return_type: 'BandoDto', produces: 'application/json', params: [{ name: 'id', type_text: 'long', binding: 'PathVariable', required: true }], file: '/Users/dev/gestionale/src/main/java/it/example/BandoController.java' },
  { id: 'UtenteController#me', methods: ['GET'], path: '/api/utenti/me', controller: 'UtenteController', handler: 'me', return_type: 'UtenteDto', produces: 'application/json', params: [], file: '/Users/dev/gestionale/src/main/java/it/example/UtenteController.java' },
];

const NOW = Date.now();

export const hirundoFixtures: Record<string, FixtureHandler> = {
  'hirundo.get_hirundo_config': CONFIG,
  'hirundo.hirundo_project_opened': CONFIG,
  'hirundo.hirundo_workspace_create': CONFIG,
  'hirundo.hirundo_workspace_update': CONFIG,
  'hirundo.hirundo_workspace_delete': CONFIG,
  'hirundo.hirundo_workspace_switch': CONFIG,
  'hirundo.hirundo_workspace_add_project': CONFIG,
  'hirundo.hirundo_workspace_remove_project': CONFIG,
  'hirundo.hirundo_project_create': '/Users/dev/new-project',
  'hirundo.hirundo_workspace_move': CONFIG,
  'hirundo.hirundo_workspace_move_project': CONFIG,
  'hirundo.hirundo_detected_roots_set': { ...CONFIG, detected_roots: [PROJECT] },
  'bennu.endpoints_detected': ENDPOINTS,
  'bennu.endpoint_schema': { request: { type_text: 'OrderDto', kind: 'object', fields: [{ name: 'id', type_text: 'long', required: true }, { name: 'note', type_text: 'String', required: false }] }, response: null },
  'bennu.get_bennu_workspaces': { active_id: 'b1', workspaces: [{ id: 'b1', name: 'Gestionale', color_idx: 0, active_project: PROJECT, projects: [{ root: PROJECT, open_files: [], active_file: '' }, { root: '/Users/dev/anagrafe', open_files: [], active_file: '' }] }] },
  ensure_product_backend: null,
  'hirundo.openapi_read': {
    title: 'Pet Store', description: 'Pets.', base_url: 'https://api.pets.example/v1', auth: { type: 'bearer', token: '{{token}}' }, tags: ['pets', 'admin'],
    operations: [
      { id: 'GET /pets', tag: 'pets', path: '/pets', summary: 'List pets', request: { ...request('List pets', 'GET', '{{baseUrl}}/pets'), params: [kv('limit', '20')] } },
      { id: 'POST /pets', tag: 'pets', path: '/pets', summary: 'Add a pet', request: request('Add a pet', 'POST', '{{baseUrl}}/pets') },
      { id: 'GET /pets/{id}', tag: 'pets', path: '/pets/{id}', summary: 'One pet', request: request('One pet', 'GET', '{{baseUrl}}/pets/{{id}}') },
      { id: 'DELETE /pets/{id}', tag: 'admin', path: '/pets/{id}', summary: 'Remove a pet', request: request('Remove a pet', 'DELETE', '{{baseUrl}}/pets/{{id}}') },
    ],
    warnings: ['OAuth 2 / OpenID Connect is used as a bearer `{{token}}`: get the token yourself and set it.'],
  },
  'hirundo.openapi_apply': { root: 'pet-store', count: 4 },
  'hirundo.soap_import': { requests: [request('GetWeather', 'POST', 'http://svc.example/weather', { soap: { version: '1.1', action: 'urn:GetWeather' } })], notes: ['The WSDL schema is not read: each operation starts with an empty body element to fill in.'] },
  'hirundo.soap_inspect': { fault: null, pretty: '<soapenv:Envelope>\n   <soapenv:Body/>\n</soapenv:Envelope>' },
  'hirundo.http_import': { entries: [{ title: 'Login', request: request('Login', 'POST', '{{host}}/api/login'), body_file: null, save_to: null, warnings: ['A JavaScript response handler (`> {% … %}`) was ignored: Hirundo\'s tests are assertions and Lua.'] }] },
  'hirundo.cookies_list': [
    { name: 'JSESSIONID', value: 'A1B2C3D4', domain: 'stg.example.it', path: '/', expires_ms: null, secure: true, http_only: true, host_only: true },
    { name: 'lang', value: 'it', domain: 'example.it', path: '/', expires_ms: Date.now() + 86400_000 * 30, secure: false, http_only: false, host_only: false },
  ],
  'hirundo.cookie_delete': null,
  'hirundo.cookies_clear': null,
  'hirundo.sse_start': { outcome: 'started' },
  'hirundo.sse_stop': true,
  'hirundo.schedules_list': [
    { id: 's1', name: 'Smoke staging', target: 'bandi', env_id: 'staging', recurrence: { kind: 'daily', hour: 2, minute: 0 }, missed: 'skip', enabled: true, created_ms: NOW - 86400_000 * 5, last_run_ms: NOW - 3600_000 * 9, last_result: 'ok', last_summary: '5 ok, 0 ko', next_run_ms: NOW + 3600_000 * 15 },
    { id: 's2', name: 'Carico leggero', target: 'bandi', env_id: '', recurrence: { kind: 'weekly', days: [0, 1, 2, 3, 4], hour: 8, minute: 0 }, missed: 'run_once', enabled: true, created_ms: NOW - 86400_000 * 9, last_run_ms: NOW - 86400_000, last_result: 'failed', last_summary: '4 ok, 1 ko', next_run_ms: NOW + 3600_000 * 20 },
  ],
  'hirundo.schedule_save': null,
  'hirundo.schedule_delete': null,
  'hirundo.schedules_due': [],
  'hirundo.schedule_ran': null,
  'hirundo.runs_list': [
    { at_ms: NOW - 3600_000 * 9, label: 'Bandi', env_name: 'Staging', source: 'Schedule · Smoke staging', duration_ms: 1900, rows: [
      { path: 'bandi/lista-bandi.toml', name: 'Lista bandi', method: 'GET', status: 200, time_ms: 184, passed: true, problems: [] },
      { path: 'bandi/crea-bando.toml', name: 'Crea bando', method: 'POST', status: 201, time_ms: 240, passed: true, problems: [] },
      { path: 'bandi/aggiorna-bando.toml', name: 'Aggiorna bando', method: 'PUT', status: 409, time_ms: 120, passed: false, problems: ['status == 200 — got 409'] },
    ] },
  ],
  'hirundo.run_record': null,
  'hirundo.runs_clear': null,
  'hirundo.collections_tree': [
    {
      path: 'bandi', name: 'Bandi', order: 0, description: '', auth: 'Bearer',
      children: [
        { kind: 'request', path: 'bandi/lista-bandi.toml', name: 'Lista bandi', method: 'GET', order: 0, error: null },
        { kind: 'request', path: 'bandi/crea-bando.toml', name: 'Crea bando', method: 'POST', order: 1, error: null },
        { kind: 'request', path: 'bandi/aggiorna-bando.toml', name: 'Aggiorna bando', method: 'PUT', order: 2, error: null },
        { kind: 'request', path: 'bandi/elimina-bando.toml', name: 'Elimina bando', method: 'DELETE', order: 3, error: null },
        {
          kind: 'folder', path: 'bandi/auth', name: 'Auth', order: 4, auth: 'Inherit',
          children: [{ kind: 'request', path: 'bandi/auth/login.toml', name: 'Login', method: 'POST', order: 0, error: null }],
        },
      ],
    },
  ],
  'hirundo.env_state': {
    types: TYPES,
    environments: [
      { id: 'local', name: 'Local', type_id: 'local', vars: [kv('baseUrl', 'http://localhost:8080')] },
      { id: 'staging', name: 'Staging', type_id: 'staging', vars: [kv('baseUrl', 'https://stg.example.it'), kv('token', '@secret')] },
      { id: 'prod', name: 'Produzione', type_id: 'prod', vars: [kv('baseUrl', 'https://api.example.it'), kv('token', '@secret')] },
    ],
    local: { active_env: 'staging', vars: [] },
  },
  'hirundo.request_get': LIST,
  'hirundo.request_save': 'bandi/lista-bandi.toml',
  'hirundo.meta_get': { name: 'Bandi', order: 0, description: '', auth: { type: 'bearer', token: '{{token}}' }, headers: [], vars: [kv('page', '0')], no_cookies: false },
  'hirundo.request_preview': {
    method: 'GET', url: 'https://stg.example.it/api/bandi?stato=APERTO&page=0',
    headers: [['Accept', 'application/json'], ['Authorization', 'Bearer ••••••']], body: null, unresolved: [], missing_secrets: [],
    env: { id: 'staging', name: 'Staging', type_id: 'staging', type_name: 'Staging', color: '#d29922', confirm_mutating: false }, needs_confirmation: false,
  },
  'hirundo.request_send': { outcome: 'sent', response: RESPONSE },
  'hirundo.tests_run': {
    checks: [
      { line: 'status == 200', passed: true, detail: '' },
      { line: 'time < 500ms', passed: true, detail: '' },
      { line: 'body.content[*].stato == "APERTO"', passed: true, detail: '' },
    ],
    set_vars: [], logs: [],
  },
  'hirundo.secret_account': 'gestionale|environment:staging|token',
  'hirundo.kv_parse': [kv('Accept', 'application/json')],
  'hirundo.kv_format': 'Accept: application/json',
  'hirundo.curl_import': request('Importata', 'POST', 'https://api.example.com/items'),
  'hirundo.curl_export': "curl -X GET 'https://stg.example.it/api/bandi?stato=APERTO&page=0' \\\n  -H 'Accept: application/json'",
  hirundo_has_secret: true,
};

export { RESPONSE as hirundoSampleResponse };
