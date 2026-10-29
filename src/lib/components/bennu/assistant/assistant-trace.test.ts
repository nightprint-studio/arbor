import { describe, expect, it } from 'vitest';
import { findStackTraces, tracePrompt } from './assistant-trace';

const JAVA = [
  'INFO  starting',                                                                   // 0
  'java.lang.NullPointerException: Cannot invoke "String.length()" because "s" is null', // 1
  '\tat com.acme.MailService.send(MailService.java:42)',                              // 2
  '\tat com.acme.MailController.register(MailController.java:18)',                    // 3
  'Caused by: java.io.IOException: boom',                                             // 4
  '\tat com.acme.Net.open(Net.java:7)',                                               // 5
  '\t... 12 more',                                                                    // 6
  'INFO  done',                                                                       // 7
];

describe('finding stack traces in output', () => {
  it('takes the exception line, its frames and its causes as one trace', () => {
    expect(findStackTraces(JAVA)).toEqual([{ head: 1, end: 6 }]);
  });

  it('finds several traces and leaves ordinary lines alone', () => {
    const lines = [...JAVA, 'Exception in thread "main" java.lang.IllegalStateException: x', '\tat a.B.c(B.java:1)'];
    expect(findStackTraces(lines)).toEqual([{ head: 1, end: 6 }, { head: 8, end: 9 }]);
  });

  it('does not take a line that merely mentions an exception for a trace', () => {
    expect(findStackTraces(['Caught an IllegalArgumentException and moved on', 'next line'])).toEqual([]);
  });

  it('needs at least one frame', () => {
    expect(findStackTraces(['java.lang.RuntimeException: lone'])).toEqual([]);
  });

  it('reads a Rust panic with its backtrace', () => {
    const lines = [
      "thread 'main' panicked at src/main.rs:5:9:",
      'index out of bounds',
      'stack backtrace:',
      '   0: rust_begin_unwind',
      '   1: app::run',
      '             at ./src/main.rs:5:9',
      'note: Some details are omitted',
    ];
    expect(findStackTraces(lines)).toEqual([{ head: 0, end: 5 }]);
  });
});

describe('the request the button sends', () => {
  it('carries the trace itself and asks for the cause, not a guess', () => {
    const p = tracePrompt(JAVA, { head: 1, end: 6 });
    expect(p).toContain('NullPointerException');
    expect(p).toContain('MailService.java:42');
    expect(p).not.toContain('INFO  starting');
    expect(p).toContain('own files');
    expect(p).toContain('edit_file');
  });

  it('cuts a very deep trace and says how much was left out', () => {
    const lines = ['java.lang.StackOverflowError', ...Array.from({ length: 400 }, (_, i) => `\tat a.B.f(B.java:${i})`)];
    const p = tracePrompt(lines, { head: 0, end: 400 });
    expect(p).toContain('281 more lines');
    expect(p.length).toBeLessThan(8000);
  });
});
