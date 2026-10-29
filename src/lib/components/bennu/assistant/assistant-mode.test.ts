import { describe, expect, it } from 'vitest';
import { MODES, modeAppliesEdits, modeInfo, nextMode, parseMode } from './assistant-mode';

describe('parseMode', () => {
  it('reads the saved words, the old one included', () => {
    expect(parseMode('manual')).toBe('manual');
    expect(parseMode('accept_edits')).toBe('accept_edits');
    expect(parseMode('autonomous')).toBe('autonomous');
    expect(parseMode('auto')).toBe('accept_edits');
    expect(parseMode(' Auto ')).toBe('accept_edits');
  });

  it('falls back to the cautious mode for anything else', () => {
    expect(parseMode('')).toBe('manual');
    expect(parseMode(undefined)).toBe('manual');
    expect(parseMode('yolo')).toBe('manual');
  });

  it('reads back every word the list writes', () => {
    for (const m of MODES) expect(parseMode(m.id)).toBe(m.id);
  });
});

describe('the modes', () => {
  it('apply edits in every mode but Manual', () => {
    expect(modeAppliesEdits('manual')).toBe(false);
    expect(modeAppliesEdits('accept_edits')).toBe(true);
    expect(modeAppliesEdits('autonomous')).toBe(true);
  });

  it('mark only Autonomous as the one to be careful with', () => {
    expect(MODES.filter((m) => m.tone === 'warning').map((m) => m.id)).toEqual(['autonomous']);
    expect(modeInfo('autonomous').description).toMatch(/build/);
  });

  it('cycle in order and wrap', () => {
    expect(nextMode('manual')).toBe('accept_edits');
    expect(nextMode('accept_edits')).toBe('autonomous');
    expect(nextMode('autonomous')).toBe('manual');
  });
});
