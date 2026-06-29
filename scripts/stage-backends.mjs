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
const BACKENDS = ['corvus-be', 'merula-be'];
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
