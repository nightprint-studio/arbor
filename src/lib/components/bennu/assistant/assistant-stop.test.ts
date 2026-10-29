import { describe, expect, it } from 'vitest';
import { cardOf, stopPrompt, withCard, withoutCard, type StopEventPayload } from './assistant-stop';

const event = (conversation: string, hit: number): StopEventPayload => ({
  conversation_id: conversation, session_id: 's', hit, card_title: 'Stopped at Order.java:42 in OrderService.total', reason: 'breakpoint', text: 'note',
});

describe('stop cards', () => {
  it('reads the event', () => {
    expect(cardOf(event('c1', 3))).toMatchObject({ conversationId: 'c1', hit: 3, title: 'Stopped at Order.java:42 in OrderService.total' });
  });

  it('keeps one card per conversation, the newest', () => {
    let cards = withCard([], cardOf(event('c1', 1)));
    cards = withCard(cards, cardOf(event('c2', 1)));
    cards = withCard(cards, cardOf(event('c1', 2)));
    expect(cards.map((c) => [c.conversationId, c.hit])).toEqual([['c2', 1], ['c1', 2]]);
  });

  it('drops the card of a conversation', () => {
    const cards = withCard(withCard([], cardOf(event('c1', 1))), cardOf(event('c2', 1)));
    expect(withoutCard(cards, 'c1').map((c) => c.conversationId)).toEqual(['c2']);
  });
});

describe('the prompt that starts a run', () => {
  it('names the place and points at the note', () => {
    const p = stopPrompt({ title: 'Stopped at A.java:1 in A.m' });
    expect(p).toContain('Stopped at A.java:1 in A.m');
    expect(p).toContain('note above');
  });
});
