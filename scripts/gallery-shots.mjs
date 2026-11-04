#!/usr/bin/env node
// Screenshots of gallery scenes, at a fixed size, for reviews and release notes.
//
//   yarn gallery                                   # in another terminal: serves http://localhost:1420
//   node scripts/gallery-shots.mjs [out-dir] [scene-id ...]
//
// With no scene ids it takes the list in SCENES below. It drives the Chrome / Edge already
// installed on the machine through `playwright-core` (no browser is downloaded); set
// GALLERY_URL to point at another port and GALLERY_BROWSER to a browser executable.
import { existsSync, mkdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { chromium } from 'playwright-core';

const BASE = process.env.GALLERY_URL ?? 'http://localhost:1420';
const out = resolve(process.argv[2] ?? '.gallery-shots');
const ids = process.argv.slice(3);

/** The scenes worth looking at when a release changes the UI. Edit as features land. */
const SCENES = [
  'corvus/plan/PlanModal',
  'corvus/plan/PlansTab',
  'corvus/mr/CreateMrModal[plan]',
  'bennu/BennuRegexModal',
  'bennu/encoding/BennuEncodingConvertModal',
  'bennu/encoding/BennuReinterpretModal',
];

/** Interactions a scene needs before it is worth a picture (the scene itself can only seed stores). */
const BEFORE = {
  'bennu/BennuRegexModal': async (page) => { await page.locator('textarea').first().fill('Released 2026-11-03, tagged 1999-12-31 and 2024-02-29.'); await page.waitForTimeout(600); },
};

const BROWSERS = [
  process.env.GALLERY_BROWSER,
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  '/usr/bin/google-chrome',
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
].filter(Boolean);
const executablePath = BROWSERS.find((p) => existsSync(p));
if (!executablePath) throw new Error('no Chrome / Edge found: set GALLERY_BROWSER');

mkdirSync(out, { recursive: true });
const browser = await chromium.launch({ executablePath });
const page = await (await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1.5 })).newPage();

for (const id of ids.length ? ids : SCENES) {
  const url = `${BASE}/dev-gallery?mock&scene=${encodeURIComponent(id)}`;
  await page.goto(url, { waitUntil: 'load' });
  await page.waitForTimeout(1500); // the scene's own `settle` is shorter than this
  await BEFORE[id]?.(page);
  const file = join(out, id.replace(/[^A-Za-z0-9_-]+/g, '__') + '.png');
  await page.screenshot({ path: file });
  console.log(file);
}
await browser.close();
