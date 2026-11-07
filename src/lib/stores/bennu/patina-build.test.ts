import { describe, expect, it } from 'vitest';
import type { PatinaBuildDiagnostic, PatinaBuildReport } from '$lib/types/bennu';
import { countProblems, isPatinaPath, patinaHeadline, toBuildDiagnostics } from './patina-build';

function diag(over: Partial<PatinaBuildDiagnostic> = {}): PatinaBuildDiagnostic {
  return {
    severity: 'error', origin: 'compiler', code: 'compile.unsupported', file: '/m/script/main.ptn',
    line: 3, col: 23, start: 69, end: 76, message: 'method `fx::sin` of the base library', ...over,
  };
}

function report(over: Partial<PatinaBuildReport> = {}): PatinaBuildReport {
  return {
    ok: true, mod_id: 'marmo', mod_root: '/m', diagnostics: [], files: 3, functions: 12, entries: 1,
    entry_list: ['hook voriz::events::StepOn'], schemas: 2, schema_list: [], capabilities: [], imports: [],
    dependencies: [], warnings: [], elapsed_ms: 38, summary: '3 file(s), 12 function(s), 1 entry, 2 schema(s) in 38 ms',
    ...over,
  };
}

describe('isPatinaPath', () => {
  it('knows a Patina source by its extension, whatever the case or the slash', () => {
    expect(isPatinaPath('/m/script/brains/shy.ptn')).toBe(true);
    expect(isPatinaPath('C:\\m\\script\\A.PTN')).toBe(true);
  });

  it('is not fooled by a name that merely contains it', () => {
    expect(isPatinaPath('/m/script/a.ptn.bak')).toBe(false);
    expect(isPatinaPath('/m/manifest.ron')).toBe(false);
    expect(isPatinaPath('')).toBe(false);
    expect(isPatinaPath(null)).toBe(false);
    expect(isPatinaPath(undefined)).toBe(false);
  });
});

describe('toBuildDiagnostics', () => {
  it('keeps the place and leads the message with the stable code', () => {
    const out = toBuildDiagnostics(report({ ok: false, diagnostics: [diag()] }));
    expect(out).toEqual([
      { file: '/m/script/main.ptn', line: 3, col: 23, severity: 'error', message: '[compile.unsupported] method `fx::sin` of the base library' },
    ]);
  });

  it('carries a problem that has no file (a missing dependency) without inventing one', () => {
    const out = toBuildDiagnostics(report({ ok: false, diagnostics: [diag({ file: null, line: null, col: null, origin: 'dependency', code: 'build.dependency_not_found', message: 'dependency `marmo` not found' })] }));
    expect(out[0].file).toBeNull();
    expect(out[0].line).toBeNull();
  });
});

describe('countProblems and patinaHeadline', () => {
  it('counts errors and warnings apart, and notes as neither', () => {
    const r = report({ ok: false, diagnostics: [diag(), diag(), diag({ severity: 'warning' }), diag({ severity: 'note' })] });
    expect(countProblems(r)).toEqual({ errors: 2, warnings: 1 });
  });

  it('says success with the summary, and mentions warnings only when there are some', () => {
    expect(patinaHeadline(report())).toBe('Patina build of marmo succeeded — 3 file(s), 12 function(s), 1 entry, 2 schema(s) in 38 ms');
    expect(patinaHeadline(report({ diagnostics: [diag({ severity: 'warning' })] }))).toContain('succeeded with 1 warning —');
  });

  it('says failure with the problem counts and no summary of what was not built', () => {
    const r = report({ ok: false, diagnostics: [diag(), diag(), diag({ severity: 'warning' })] });
    expect(patinaHeadline(r)).toBe('Patina build of marmo failed — 2 errors, 1 warning');
    expect(patinaHeadline(report({ ok: false, mod_id: '', diagnostics: [diag()] }))).toBe('Patina build of the mod failed — 1 error');
  });
});
