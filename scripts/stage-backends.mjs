// Stage the product backend binaries into `src-tauri/backends/` so Tauri bundles
// them (as resources) into a dedicated `backends/` subfolder of the installed
// app. Run after building the backends in release; before `tauri build`.
//
// Dev doesn't need this: `cargo build -p <name>` co-locates the binary beside the
// launcher in `target/debug/`, where the launcher's resolver finds it directly.
//
// Node built-ins only — no dependencies.

import { mkdirSync, copyFileSync, existsSync } from 'node:fs';

const EXE = process.platform === 'win32' ? '.exe' : '';
// Being in `backends:release` is NOT enough: that only *builds* a backend. A
// binary missing from this list is never copied into `src-tauri/backends/`, which
// is what `tauri.conf.json`'s `resources` bundles — so it works in dev (the
// launcher finds it beside itself in `target/debug/`) and silently has no backend
// in an installed app.
//
// That failure mode is worth spelling out, because it cost a bug report: the
// product's window still opens (the shell serves the webview), so it looks alive
// while every one of its RPCs answers `BackendNotRunning`. For Bennu that read as
// "workspaces are added but projects are not, and closing loses the workspaces" —
// the workspace list is frontend state, and the two things that needed the
// backend were opening a project and writing `workspace.toml`.
//
// So: this list must name **every** backend the app can spawn. Keep it in step
// with `backends:release` in package.json and with the `ensure_*_be` spawners in
// `src-tauri/src/ipc/mod.rs`.
const BACKENDS = [
  'corvus-be',
  'merula-be',
  'sitta-be',
  'picus-be',
  'garrulus-be',
  'bennu-be',
  'tyto-be',
];
const destDir = 'src-tauri/backends';

mkdirSync(destDir, { recursive: true });

for (const name of BACKENDS) {
  const src = `src-tauri/target/release/${name}${EXE}`;
  if (!existsSync(src)) {
    console.error(`stage-backends: ${src} not found — run "npm run backends:release" first`);
    process.exit(1);
  }
  copyFileSync(src, `${destDir}/${name}${EXE}`);
  console.log(`stage-backends: ${name}${EXE} → ${destDir}/`);
}
