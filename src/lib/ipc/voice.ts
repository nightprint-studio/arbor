/**
 * Voice IPC — local speech-to-text, served by `voice-be`.
 *
 * Platform-level: any product window can borrow it. `voice-be` starts on first use and exits by
 * itself when idle, so every call goes through {@link voice}, which makes sure it is up first
 * (a no-op once confirmed, and re-checked after the backend went down).
 */

import { rpc, ensureBackend } from './rpc';

export type VoiceReason = 'push_to_talk' | 'toggle' | 'wake' | 'test';

export interface WakeConfig {
  enabled: boolean;
  phrases: string[];
  in_background: boolean;
  auto_disarm_minutes: number;
  silence_ms: number;
  detector_model: string;
}

export interface VoiceConfig {
  model: string;
  custom_model_path: string;
  language: string;
  microphone: string;
  /** Show text while the user is still speaking (falls back to on-release for slow models). */
  live_text: boolean;
  vocabulary_hints: boolean;
  idle_minutes: number;
  max_seconds: number;
  overlay: boolean;
  wake: WakeConfig;
}

export interface VoiceStatus {
  state: 'idle' | 'listening' | 'transcribing';
  armed: boolean;
  listening_id: number | null;
  mic_open: boolean;
  model_loaded: number;
  /** This build contains the speech engine. */
  engine: boolean;
  model_ready: boolean;
  /** The model in use (catalogue id); "" for none or the user's own file. */
  selected: string;
  /** The current dictation shows text while the user speaks. */
  streaming: boolean;
  /** Why it does not, when it was asked to. */
  streaming_note: string | null;
  pid: number;
}

export type Comfort = 'instant' | 'comfortable' | 'slow' | 'too_slow';

export interface VoiceTranscript {
  id: number;
  reason: VoiceReason;
  text: string;
  audio_secs: number;
  took_secs: number;
  rtf: number;
  comfort: Comfort | null;
  /** The part not already delivered as live text (== text when nothing streamed). */
  tail: string;
  /** Live pieces were announced while the user spoke. */
  streamed: boolean;
  model: string;
  /** Nothing was heard; nothing was transcribed. */
  silent: boolean;
  error: string | null;
}

export interface VoiceDevice {
  name: string;
  default: boolean;
}

export interface VoiceModel {
  id: string;
  label: string;
  file: string;
  size_bytes: number;
  sha256: string;
  english_only: boolean;
  cost: number;
  accuracy: number;
  quantized: boolean;
  installed: boolean;
  installing: boolean;
  recommended: boolean;
  /** Expected seconds of work per second of speech on this PC. */
  rtf: number;
  /** `rtf` was measured on this PC, not estimated. */
  rtf_measured: boolean;
  /** Text can appear while speaking with this model. */
  live_ok: boolean;
  url: string;
}

export interface VoiceModelList {
  models: VoiceModel[];
  hardware: { cores: number; logical: number; ram_gb: number; gpu: boolean };
  recommendation: { id: string; reason: string };
  engine: boolean;
  free_disk_bytes: number | null;
  models_dir: string;
  /** The model in use (catalogue id), "" for none / own file. */
  selected: string;
  custom: boolean;
}

export interface VoiceVocabulary {
  project: string;
  active: string[];
  neighbours: string[];
  recent: string[];
}

/** Call `voice-be`, starting it first when it is not running. */
export async function voice<R>(method: string, params: Record<string, unknown> = {}): Promise<R> {
  await ensureBackend('voice');
  return rpc<R>('voice', method, params);
}

export const voiceApi = {
  status: () => voice<VoiceStatus>('voice_status'),
  getConfig: () => voice<VoiceConfig>('voice_get_config'),
  /** Change some settings; only the named fields are touched. */
  setConfig: (patch: Partial<VoiceConfig>) => voice<VoiceConfig>('voice_set_config', { patch }),
  selectModel: (id: string) => voice<VoiceConfig>('voice_select_model', { id }),
  devices: () => voice<VoiceDevice[]>('voice_list_devices'),
  models: (language?: string) => voice<VoiceModelList>('voice_list_models', { language }),
  installModel: (id: string) => voice<void>('voice_install_model', { id }),
  removeModel: (id: string) => voice<void>('voice_remove_model', { id }),
  setVocabulary: (vocab: VoiceVocabulary) => voice<void>('voice_set_vocabulary', { vocab }),
  start: (reason: VoiceReason) => voice<number>('voice_start_listening', { reason }),
  stop: () => voice<VoiceTranscript>('voice_stop_listening'),
  cancel: () => voice<void>('voice_cancel'),
  test: (model: string, seconds = 5) => voice<VoiceTranscript>('voice_test_model', { model, seconds }),
  arm: () => voice<void>('voice_arm'),
  disarm: () => voice<void>('voice_disarm'),
  shutdown: () => voice<void>('voice_shutdown'),
};
