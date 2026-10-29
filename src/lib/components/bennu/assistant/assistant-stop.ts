/** The pure parts of "the program the assistant is debugging stopped": the cards and the prompt that starts a run. */

/** A stop announced in the thread of a conversation that was idle. */
export interface StopCard {
  conversationId: string;
  sessionId: string;
  hit: number;
  /** `Stopped at Order.java:42 in OrderService.total`. */
  title: string;
  reason: string;
  /** The snapshot as the assistant will read it (capped and masked by the backend). */
  text: string;
}

/** The event the backend emits (`arbor://bennu/assistant-stop`). */
export interface StopEventPayload {
  conversation_id: string;
  session_id: string;
  hit: number;
  card_title: string;
  reason: string;
  text: string;
}

export function cardOf(e: StopEventPayload): StopCard {
  return { conversationId: e.conversation_id, sessionId: e.session_id, hit: e.hit, title: e.card_title, reason: e.reason, text: e.text };
}

/** One card per conversation: a newer stop replaces the older, since only the latest state of the program matters. */
export function withCard(cards: StopCard[], card: StopCard): StopCard[] {
  return [...cards.filter((c) => c.conversationId !== card.conversationId), card];
}

export function withoutCard(cards: StopCard[], conversationId: string): StopCard[] {
  return cards.filter((c) => c.conversationId !== conversationId);
}

/** The short message that starts a run on a stop; the snapshot itself is already queued for the assistant by Bennu. */
export function stopPrompt(card: Pick<StopCard, 'title'>): string {
  return `${card.title}. The program you are debugging is suspended; Bennu's note above has where it stopped and the values. Look at it and carry on with what we were doing.`;
}
