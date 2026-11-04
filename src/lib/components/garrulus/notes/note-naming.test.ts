import { describe, expect, it } from 'vitest';
import {
  folderOf,
  iconKey,
  normalizeNotePath,
  notePathProblem,
  stemFromTitle,
  stemRange,
  stripCursorMarker,
  todayStamp,
  withNoteExtension,
} from './note-naming';

describe('stemFromTitle', () => {
  it('keeps a title readable and drops what a filesystem refuses', () => {
    expect(stemFromTitle('Bug: crash all\'avvio?')).toBe("Bug crash all'avvio");
  });
  it('collapses whitespace and strips leading dots', () => {
    expect(stemFromTitle('  ..hidden   note ')).toBe('hidden note');
  });
  it('is empty for a title with nothing nameable in it', () => {
    expect(stemFromTitle(' ?*: ')).toBe('');
  });
});

describe('paths', () => {
  it('normalises separators', () => {
    expect(normalizeNotePath('  /bugs\\\\2026//a.md ')).toBe('bugs/2026/a.md');
  });
  it('adds .md only when missing', () => {
    expect(withNoteExtension('a')).toBe('a.md');
    expect(withNoteExtension('a.MD')).toBe('a.MD');
    expect(withNoteExtension('a.markdown')).toBe('a.markdown');
  });
  it('splits folder and stem', () => {
    expect(folderOf('bugs/a.md')).toBe('bugs/');
    expect(folderOf('a.md')).toBe('');
    expect(stemRange('bugs/a note.md')).toEqual([5, 11]);
    expect(stemRange('plain')).toEqual([0, 5]);
  });
});

describe('notePathProblem', () => {
  const taken = new Set(['bugs/Crash.md', 'Home.md']);

  it('accepts a fresh name', () => {
    expect(notePathProblem('bugs/other', taken)).toBeNull();
  });
  it('refuses an empty name and a bare folder', () => {
    expect(notePathProblem('  ', taken)).not.toBeNull();
    expect(notePathProblem('bugs/', taken)).not.toBeNull();
    expect(notePathProblem('.md', taken)).not.toBeNull();
  });
  it('refuses leaving the vault and hidden names', () => {
    expect(notePathProblem('../x', taken)).not.toBeNull();
    expect(notePathProblem('.arbor/x', taken)).not.toBeNull();
  });
  it('refuses characters Windows rejects', () => {
    expect(notePathProblem('a?b', taken)).not.toBeNull();
  });
  it('compares existing notes case-insensitively, extension implied', () => {
    expect(notePathProblem('BUGS/crash', taken)).not.toBeNull();
    expect(notePathProblem('home.md', taken)).not.toBeNull();
  });
  it('lets a note keep its own name, in any case', () => {
    expect(notePathProblem('bugs/crash', taken, 'bugs/Crash.md')).toBeNull();
  });
});

describe('misc', () => {
  it('formats a local date', () => {
    expect(todayStamp(new Date(2026, 0, 5, 23, 59))).toBe('2026-01-05');
  });
  it('removes the cursor marker wherever it sits', () => {
    expect(stripCursorMarker('1. {{cursor}}\n2. {{ cursor }}')).toBe('1. \n2. ');
  });
});

describe('iconKey', () => {
  it('maps lucide kebab names to the icon table keys', () => {
    expect(iconKey('bug')).toBe('Bug');
    expect(iconKey('file-text')).toBe('FileText');
    expect(iconKey('CalendarDays')).toBe('CalendarDays');
    expect(iconKey('')).toBe('');
  });
});
