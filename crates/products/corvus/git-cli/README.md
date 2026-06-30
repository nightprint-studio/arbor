# corvus-git-cli

Tauri-free resolution of the system `git` executable, shared by the Arbor shell
(`src-tauri`) and the headless `corvus-be` process.

Arbor shells out to `git` for the operations libgit2 handles poorly (rebase,
stash, submodule, recovery snapshots). This crate decides **which** binary those
calls run and builds the pre-configured `Command`:

1. an explicit path from the app config (`[git] executable_path`),
2. the first `git` on `PATH`,
3. a bundled **PortableGit** under `arbor_config_path("git")` (Windows only,
   populated by `download_portable`).

`detect()` writes a process-global detection state; `snapshot()` reads it;
`command()` builds a console-less `Command` for the resolved binary. Each process
owns its own instance and self-detects — nothing is shared across the process
boundary.

The keyring-coupled HTTP auth-arg injection (`http_auth_args_for_url`) is **not**
here: it reads stored credentials, which never cross into a headless backend, so
it stays shell-side (`src-tauri/src/git/auth_args.rs`).

Public API is via `prelude`: `snapshot`, `command`, `detect`, `verify`,
`set_path`, `clear_override`, `portable_dir`, `download_portable`,
`download_supported`, `request_download_cancel`, `GitCliState`,
`DownloadProgress`, `GitCliError`.

## Depends on

`arbor-core` (the profile-aware config path + HTTP user-agent), `arbor-process-ext`
(`NoWindowExt`), `reqwest` + `sevenz-rust2` (the PortableGit download/extract),
`serde` / `serde_json`, `tokio` (the async download), `thiserror`, `tracing`. No
Tauri, no keyring.
