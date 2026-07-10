// Codesign the dev shell binary (`arbor`) with a stable local identity so the
// macOS Keychain "Always Allow" ACL survives rebuilds — no more repeated
// password prompts when reading stored git credentials.
//
// No-op on non-macOS and when the binary isn't built yet, so it's safe to chain
// into any script on any platform. The signing identity defaults to "Arbor Dev"
// (create it once via Keychain Access → Certificate Assistant → Code Signing);
// override with ARBOR_SIGN_IDENTITY.
//
// NB: only the shell binary talks to the Keychain — the *-be backends are
// keyring-free, so they don't need signing.

import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';

if (process.platform !== 'darwin') process.exit(0);

const identity = process.env.ARBOR_SIGN_IDENTITY || 'Arbor Dev';
const candidates = ['target/debug/arbor', 'src-tauri/target/debug/arbor'];
const bin = candidates.find(existsSync);

if (!bin) {
  console.error(`[sign-macos] arbor binary not found (${candidates.join(', ')}) — build it first; skipping.`);
  process.exit(0);
}

try {
  execFileSync('codesign', ['--force', '--sign', identity, bin], { stdio: 'inherit' });
  console.log(`[sign-macos] signed ${bin} with "${identity}"`);
} catch (e) {
  // Don't block the dev launch on a signing hiccup — worst case is the
  // in-memory credential cache still limits prompts to one per session.
  console.error(`[sign-macos] codesign failed (${e.message}) — continuing unsigned.`);
}
