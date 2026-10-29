/**
 * The voice verbs of Bennu's command palette. One list, so the palette and any future menu say
 * the same things; each takes the window's `run` wrapper (closes the palette first).
 */

import { bennuVoiceStore } from '$lib/stores/bennu/voice.svelte';
import { voiceStore } from '$lib/stores/voice.svelte';

export interface VoiceCommand {
  id: string;
  title: string;
  icon: string;
  shortcut?: string;
  action: () => void;
  when: boolean;
}

export function voiceCommands(run: (fn: () => void) => void, openVoiceSettings: () => void): VoiceCommand[] {
  return [
    { id: 'voice-dictate', title: 'Voice: start dictation', icon: 'Mic', action: () => run(() => void bennuVoiceStore.toggle()), when: true },
    { id: 'voice-model', title: 'Voice: choose model', icon: 'AudioLines', action: () => run(openVoiceSettings), when: true },
    { id: 'voice-test-mic', title: 'Voice: test microphone', icon: 'Mic', action: () => run(openVoiceSettings), when: true },
    {
      id: 'voice-arm',
      title: voiceStore.armed ? 'Voice: disarm wake word' : 'Voice: arm wake word (keeps the microphone open)',
      icon: 'Ear',
      action: () => run(() => void voiceStore.toggleArm()),
      when: true,
    },
  ];
}
