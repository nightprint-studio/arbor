# Security Policy

Arbor is a desktop Git client that handles credentials, OAuth tokens, repository data, and runs sandboxed Lua plugins. The areas that matter most from a security standpoint are:

- The plugin sandbox and its permissions model (filesystem, network, git read/write, env, services)
- OAuth flows and token storage in the OS keyring
- The CI/CD and security-dashboard integrations (GitHub, GitLab) and the tokens they use
- IPC commands and the boundary between the frontend and the Rust backend

## Reporting a vulnerability

Please report suspected vulnerabilities **privately**, not as public GitHub issues.

Two channels:

1. **GitHub Private Vulnerability Reporting** (preferred). Open a report at [Security → Report a vulnerability](https://github.com/nightprint-studio/arbor/security/advisories/new). It creates a private discussion thread that only maintainers can see.
2. **Email.** `nightprint.studio@gmail.com` if you'd rather not use GitHub. No PGP key is set up for now; plain mail is fine, just keep the report concise.

Useful information to include:

- A description of the issue and where it lives (component, file, plugin name if relevant)
- Steps to reproduce, or a proof-of-concept if you have one
- The Arbor version and platform (Windows / macOS / Linux)
- Your assessment of impact and severity (best guess is fine)

## What to expect

- Acknowledgement within a few days. Arbor is maintained by a single person, so response times are best-effort.
- A short back-and-forth to confirm the issue and agree on a coordinated disclosure timeline. The target is 90 days from initial report to public disclosure; the window is flexible based on severity and complexity.
- Credit in the release notes once a fix ships, unless you prefer to stay anonymous.

## Scope

**In scope** — vulnerabilities in:

- Arbor's Rust backend, IPC commands, and authentication flows
- Plugin sandbox bypasses (any way for a plugin to gain capabilities its manifest doesn't declare)
- Plugins published in the official [arbor-extensions](https://github.com/nightprint-studio/arbor-extensions) marketplace registry
- Theme files in `themes/` if they can lead to code execution or token exfiltration

**Out of scope:**

- Plugins from custom user-added sources or unofficial mirrors. Please report those to the plugin's own maintainer.
- Issues that require an attacker to already have full control of the user's machine, the OS keyring, or the Arbor process memory. Local privilege boundaries we don't claim to enforce are out of scope.
- Vulnerabilities in upstream dependencies (libgit2, mlua, Tauri, Svelte, …) that don't have meaningful Arbor-side amplification. Report those upstream; pinging Arbor in parallel is welcome, and the fix will be coordinated rather than re-patched independently here.
- Findings from automated scanners that don't translate into a concrete exploitable scenario.

## Disclosure

Once a fix is in a release and users have had a reasonable window to update, the advisory is published with credit to the reporter (unless anonymity was requested). For critical issues, coordination with package maintainers and downstream distributors may happen before going public.
