/** Mock answers for the "merula" area — see fixtures/index.ts. Keys: Tauri command, or `program.method` for rpc. */
import type { FixtureHandler } from './index';

const HOME = '/Users/dev';
export const MERULA_PROJECT = `${HOME}/music/nobody-digs-anymore`;

const file = (rel: string, library = false) => ({
  path: `${MERULA_PROJECT}/${rel}`, rel, name: rel.split('/').pop()!, library,
});

export const merulaFixtures: Record<string, FixtureHandler> = {
  'merula.get_merula_state': {
    recent_projects: [MERULA_PROJECT, `${HOME}/music/sketches`, `${HOME}/music/live-set-2026`],
    last_project: MERULA_PROJECT,
    layout: { left_panel: 'files', bottom_panel: 'console', right_panel: 'inspector', collapse_viz: false, collapse_editor: false },
    favorite_sounds: ['bd', 'RolandTR909_hh'],
    recent_sounds: ['bd', 'sd', 'hh', 'piano'],
    workspaces: [
      { id: 'w1', name: 'Album', color_idx: 0, project_paths: [MERULA_PROJECT, `${HOME}/music/live-set-2026`] },
      { id: 'w2', name: 'Experiments', color_idx: 3, project_paths: [`${HOME}/music/sketches`] },
    ],
    active_workspace: 'w1',
  },
  'merula.set_merula_state': null,
  'merula.merula_open_project': {
    path: MERULA_PROJECT, name: 'Nobody Digs Anymore', audience: 'Players of a mole-themed idle game',
    files: [file('intro.merula'), file('theme_dig.merula'), file('boss_rockfall.merula'), file('lib/drums.merula', true)],
  },
  'merula.get_merula_project_tabs': { open_file_paths: [`${MERULA_PROJECT}/intro.merula`], active_file_path: `${MERULA_PROJECT}/intro.merula` },
  'merula.get_merula_config': {
    default_octave: 4, default_cps: 0.5, log_threshold: 'info',
    render: { sample_rate: 48000, bit_depth: 'int24', tail_max_secs: 4, format: 'wav' },
    vsco_dir: null, packs_dir: null, output_device: null, skip_step_cycles: 1,
  },
  'merula.merula_audio_devices': [
    { name: 'MacBook Pro Speakers', is_default: true },
    { name: 'External Headphones', is_default: false },
    { name: 'Scarlett 2i2 USB', is_default: false },
  ],
  'merula.merula_scales': [
    { name: 'major', aliases: ['ionian'], intervals: [0, 2, 4, 5, 7, 9, 11] },
    { name: 'minor', aliases: ['aeolian'], intervals: [0, 2, 3, 5, 7, 8, 10] },
    { name: 'dorian', aliases: [], intervals: [0, 2, 3, 5, 7, 9, 10] },
    { name: 'mixolydian', aliases: [], intervals: [0, 2, 4, 5, 7, 9, 10] },
  ],
  'merula.merula_export_plan': [
    { path: `${MERULA_PROJECT}/intro.merula`, rel: 'intro.merula', stem: 'intro', title: 'Intro', cycles: 16, cycles_from: 'meta', error: null },
    { path: `${MERULA_PROJECT}/theme_dig.merula`, rel: 'theme_dig.merula', stem: 'theme_dig', title: 'Digging theme', cycles: 32, cycles_from: 'arrangement', error: null },
    { path: `${MERULA_PROJECT}/boss_rockfall.merula`, rel: 'boss_rockfall.merula', stem: 'boss_rockfall', title: null, cycles: 8, cycles_from: 'default', error: 'line 12: unknown function `rokc`' },
  ],
  'merula.merula_packs': [
    { id: 'vsco', name: 'VSCO Community Orchestra', description: 'Orchestral instruments, CC0.', approx_bytes: 250_000_000, installed: true, active: true, path: `${HOME}/.arbor/packs/vsco`, size_bytes: 248_000_000, sha256: null, instrument_count: 44 },
    { id: 'dirt-samples', name: 'Dirt-Samples', description: 'Classic live-coding sample set.', approx_bytes: 90_000_000, installed: false, active: false, path: '', size_bytes: 0, sha256: null, instrument_count: 0 },
  ],
  'merula.get_merula_aliases': { kick: 'bd', snare: 'sd' },
  'merula.get_merula_scratch_tabs': { tabs: [], active: null },
};
