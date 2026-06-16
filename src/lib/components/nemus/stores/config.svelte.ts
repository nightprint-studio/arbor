/**
 * nemus config store — reactive mirror of the typed `[nemus]` settings
 * (`~/.config/arbor/config.toml`), routed through the backend (never
 * localStorage; Arbor hard rule #11). Mirrors the appearance-store shape:
 * defaults render immediately, `loadConfig()` overwrites from disk (called from
 * the nemus window boot), setters persist.
 */

import {
  getNemusConfig, setNemusConfig, nemusSetOutputDevice,
  type NemusConfig, type NemusRenderConfig,
} from '$lib/ipc/nemus';

export const LOG_LEVELS = ['trace', 'debug', 'info', 'warn', 'error'] as const;
export type NemusLogThreshold = (typeof LOG_LEVELS)[number];

const DEFAULT_RENDER: NemusRenderConfig = { sample_rate: 48_000, bit_depth: 'int24', tail_max_secs: 4.0, format: 'wav' };

function createConfigStore() {
  let defaultOctave = $state(4);
  let defaultCps    = $state(0.5);
  let logThreshold  = $state<string>('info');
  let render        = $state<NemusRenderConfig>({ ...DEFAULT_RENDER });
  let vscoDir       = $state<string | null>(null);
  let packsDir      = $state<string | null>(null);
  let outputDevice  = $state<string | null>(null);
  let skipStep      = $state(1);
  let loaded        = $state(false);

  function snapshot(): NemusConfig {
    return {
      default_octave: defaultOctave,
      default_cps:    defaultCps,
      log_threshold:  logThreshold,
      render:         { ...render },
      vsco_dir:       vscoDir,
      packs_dir:      packsDir,
      output_device:  outputDevice,
      skip_step_cycles: skipStep,
    };
  }

  function persist() { void setNemusConfig(snapshot()).catch(() => {}); }

  return {
    get defaultOctave() { return defaultOctave; },
    get defaultCps()    { return defaultCps; },
    get logThreshold()  { return logThreshold; },
    get render()        { return render; },
    get vscoDir()       { return vscoDir; },
    get packsDir()      { return packsDir; },
    get outputDevice()  { return outputDevice; },
    /** Transport step distance in cycles (bars) for the step-back/forward buttons. */
    get skipStep()      { return skipStep; },
    /** Human label for the step distance (`1 cycle` / `4 cycles`). */
    get skipStepLabel() { return `${skipStep} ${skipStep === 1 ? 'cycle' : 'cycles'}`; },
    get loaded()        { return loaded; },

    async loadConfig() {
      try {
        const cfg = await getNemusConfig();
        defaultOctave = cfg.default_octave;
        defaultCps    = cfg.default_cps;
        logThreshold  = cfg.log_threshold;
        render        = { ...cfg.render };
        vscoDir       = cfg.vsco_dir;
        packsDir      = cfg.packs_dir;
        outputDevice  = cfg.output_device ?? null;
        skipStep      = cfg.skip_step_cycles ?? 1;
        loaded = true;
      } catch {
        // First-run / backend not ready — keep defaults; next call retries.
      }
    },

    setLogThreshold(level: string) {
      if (logThreshold === level) return;
      logThreshold = level;
      persist();
    },
    setDefaultOctave(n: number) {
      if (defaultOctave === n) return;
      defaultOctave = n;
      persist();
    },
    setDefaultCps(cps: number) {
      if (defaultCps === cps) return;
      defaultCps = cps;
      persist();
    },
    /** Set the transport step distance (cycles); clamped to a sane 0.25–16 range. */
    setSkipStep(cycles: number) {
      const v = Math.min(16, Math.max(0.25, cycles || 1));
      if (skipStep === v) return;
      skipStep = v;
      persist();
    },
    setVscoDir(dir: string | null) {
      if (vscoDir === dir) return;
      vscoDir = dir;
      persist();
    },
    /** Choose the audio output device (cpal name, or null = host default). Uses
     *  the dedicated command (persists + switches a live session immediately),
     *  not the generic config save. */
    setOutputDevice(name: string | null) {
      if (outputDevice === name) return;
      outputDevice = name;
      void nemusSetOutputDevice(name).catch(() => {});
    },
    setRender(next: NemusRenderConfig) {
      render = { ...next };
      persist();
    },
  };
}

export const configStore = createConfigStore();
