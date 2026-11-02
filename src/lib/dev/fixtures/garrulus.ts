/** Mock answers for the "garrulus" area — see fixtures/index.ts. Keys: Tauri command, or `program.method` for rpc. */
import type { FixtureHandler } from './index';

const HOME = '/Users/dev';
const NOW = Date.now();

export const garrulusFixtures: Record<string, FixtureHandler> = {
  'garrulus.garrulus_list_vaults': [
    { id: 'v1', path: `${HOME}/notes/work`, display_name: 'work', remote: { kind: 'git', gitRemote: 'origin', branch: 'main' }, last_opened: NOW - 3600_000 },
    { id: 'v2', path: `${HOME}/notes/personal`, display_name: 'personal', remote: null, last_opened: NOW - 5 * 86400_000 },
    { id: 'v3', path: `${HOME}/Documents/zettelkasten`, display_name: 'zettelkasten', remote: { kind: 'folder', folder: `${HOME}/Dropbox/zk` }, last_opened: NOW - 30 * 86400_000 },
  ],
  'garrulus.garrulus_remote_config': { kind: 'git', gitRemote: 'origin', branch: 'main' },
};
