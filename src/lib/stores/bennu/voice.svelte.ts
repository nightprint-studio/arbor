/**
 * Bennu's side of voice: what happens between "the user started talking" and "the answer is on
 * screen". The generic store (`voiceStore`) owns the microphone; this one owns the overlay, the
 * vocabulary handed to the recogniser and the hand-off to the assistant.
 *
 * A spoken request to the assistant is the same machinery as a typed one: the selection chips come
 * from the assistant store (`refreshSelection`), the message goes through `send`, and the
 * conversation it creates is the one the panel shows when the user continues there.
 */

import { voiceApi, type VoiceTranscript } from '$lib/ipc/voice';
import { voiceStore, type VoiceTarget } from '$lib/stores/voice.svelte';
import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
import { projectStore } from '$lib/stores/bennu/project.svelte';
import { vocabularyFor } from '$lib/components/bennu/assistant/assistant-voice';

function createBennuVoiceStore() {
  let overlayOpen = $state(false);
  let request = $state<string | null>(null);
  let problem = $state<string | null>(null);
  let wired = false;
  let unsubs: Array<() => void> = [];

  /** Selection chips + vocabulary, gathered BEFORE the focus moves anywhere. */
  async function prepare() {
    bennuAssistantStore.refreshSelection();
    if (!voiceStore.config) await voiceStore.loadConfig();
    if (voiceStore.config?.vocabulary_hints) {
      try {
        await voiceApi.setVocabulary(vocabularyFor({
          projectName: projectStore.project?.name ?? null,
          activePath: projectStore.activeFilePath ?? null,
          openPaths: projectStore.openFilePaths,
          selection: '',
        }));
      } catch { /* hints are a nicety */ }
    }
  }

  function overlayWanted(): boolean { return voiceStore.config?.overlay ?? true; }

  async function sendToAssistant(text: string, openPanel: boolean) {
    const a = bennuAssistantStore;
    if (openPanel) bennuUiStore.showRight('assistant');
    await a.start();
    a.startFresh();
    void a.send(text);
  }

  async function onTranscript(t: VoiceTranscript, target: VoiceTarget) {
    if (target !== 'overlay') return;
    if (t.error === 'cancelled') return;
    if (t.error) { problem = t.error; return; }
    if (t.silent || !t.text.trim()) { problem = 'No speech was heard.'; request = null; return; }
    problem = null;
    request = t.text.trim();
    // With the overlay off the answer simply appears in the panel.
    await sendToAssistant(request, !overlayOpen);
  }

  return {
    get overlayOpen() { return overlayOpen; },
    get request() { return request; },
    get problem() { return problem; },

    /** Attach the listeners (idempotent). Called by the host component on mount. */
    wire() {
      if (wired) return;
      wired = true;
      unsubs = [
        voiceStore.onTranscript((t, target) => { void onTranscript(t, target); }),
        voiceStore.onWake(() => {
          request = null;
          problem = null;
          if (overlayWanted()) { bennuAssistantStore.refreshSelection(); overlayOpen = true; }
        }),
      ];
    },
    unwire() { unsubs.forEach((u) => u()); unsubs = []; wired = false; },

    /** The user pressed the talk key / chose "start dictation". */
    async begin(reason: 'push_to_talk' | 'toggle') {
      if (voiceStore.busy) return;
      await prepare();
      request = null;
      problem = null;
      overlayOpen = overlayWanted();
      if (!(await voiceStore.startListening(reason, 'overlay'))) problem = voiceStore.error ?? 'The microphone could not be started.';
    },

    /** The talk key was released (or the toggle pressed again). */
    async end() { if (voiceStore.listening) await voiceStore.stopListening(); },

    async toggle() { if (voiceStore.listening) await this.end(); else await this.begin('toggle'); },

    closeOverlay() { overlayOpen = false; request = null; problem = null; },

    /** "Try again" after nothing was heard: the same as pressing the toggle key. */
    async retry() { await this.begin('toggle'); },

    /** The overlay's way out of "no model / no microphone": close it and open Settings › Voice. */
    openVoiceSettings() { this.closeOverlay(); bennuUiStore.openSettings('voice'); },

    /** "Continue in the panel": same conversation, now in the side panel. */
    continueInPanel() { bennuUiStore.showRight('assistant'); this.closeOverlay(); },

    /** The first follow-up leaves the overlay for the panel. */
    followUp(text: string) {
      bennuUiStore.showRight('assistant');
      this.closeOverlay();
      void bennuAssistantStore.send(text);
    },
  };
}

export const bennuVoiceStore = createBennuVoiceStore();
