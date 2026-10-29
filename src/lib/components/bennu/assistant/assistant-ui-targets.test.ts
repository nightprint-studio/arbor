import { describe, expect, it } from 'vitest';
import { openUi, type UiHost } from './assistant-ui-targets';

function recorder() {
  const calls: string[] = [];
  const ui: UiHost = {
    showBottom: (p) => calls.push(`bottom:${p}`),
    showRight: (p) => calls.push(`right:${p}`),
    showLeft: (p) => calls.push(`left:${p}`),
    openVulnerabilities: () => calls.push('vulnerabilities'),
    openModuleGraph: () => calls.push('graph'),
    openSsr: () => calls.push('ssr'),
    openFind: (q) => calls.push(`find:${q}`),
    openSettings: () => calls.push('settings'),
  };
  return { ui, calls };
}

describe('opening a window for the assistant', () => {
  it('opens the vulnerabilities dialog', () => {
    const { ui, calls } = recorder();
    expect(openUi(ui, 'vulnerabilities')).toBe(true);
    expect(calls).toEqual(['vulnerabilities']);
  });

  it('shows a panel rather than toggling it', () => {
    const { ui, calls } = recorder();
    openUi(ui, 'problems');
    openUi(ui, 'tests');
    expect(calls).toEqual(['bottom:problems', 'right:tests']);
  });

  it('types the query into Find in Files', () => {
    const { ui, calls } = recorder();
    openUi(ui, 'find_in_files', 'LogService');
    openUi(ui, 'find_in_files');
    expect(calls).toEqual(['find:LogService', 'find:']);
  });

  it('says so for a target it does not know', () => {
    const { ui, calls } = recorder();
    expect(openUi(ui, 'the-moon')).toBe(false);
    expect(calls).toEqual([]);
  });
});
