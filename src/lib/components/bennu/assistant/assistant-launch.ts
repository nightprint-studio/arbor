/**
 * Starting something with the assistant from somewhere else in the window.
 *
 * Every place that offers the assistant — the editor's margin mark, a comment's button, a stack trace in the
 * console — does the same four things: open the panel, make sure the assistant is loaded, begin a
 * conversation of its own (so the question does not land in the middle of one the user had open), and send.
 * One place for it, so they cannot drift apart.
 */

import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';

async function open() {
  bennuUiStore.showRight('assistant');
  await bennuAssistantStore.start();
  bennuAssistantStore.startFresh();
}

/** Open the assistant in a new conversation and send `prompt`. */
export async function askAssistant(prompt: string): Promise<void> {
  await open();
  await bennuAssistantStore.send(prompt);
}

/** Open the assistant in a new conversation with `text` in the composer, for the user to finish. */
export async function draftInAssistant(text: string): Promise<void> {
  await open();
  bennuAssistantStore.draft = text;
  bennuAssistantStore.focusComposer();
}
