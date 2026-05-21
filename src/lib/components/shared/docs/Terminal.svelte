<h1>Terminal</h1>

<p class="doc-lead">Arbor includes a built-in multi-tab terminal emulator powered by <strong>xterm.js</strong> and native PTY — ConPTY on Windows, POSIX PTY on Linux/macOS. No window-switching required.</p>

<h2>Opening the terminal</h2>
<table class="shortcuts-table">
  <thead><tr><th>Action</th><th>How</th></tr></thead>
  <tbody>
    <tr><td>Toggle terminal panel</td><td><kbd>Ctrl+`</kbd> or the terminal icon in the Activity Bar</td></tr>
    <tr><td>Open a new tab immediately</td><td><kbd>Ctrl+Shift+`</kbd></td></tr>
    <tr><td>New tab in default shell</td><td>Click <strong>+</strong> in the terminal tab bar</td></tr>
    <tr><td>Pick a shell from the list</td><td>Click the <strong>▾</strong> dropdown next to <strong>+</strong></td></tr>
    <tr><td>Close tab</td><td>Click <strong>×</strong> on the tab, or type <code>exit</code> in the shell</td></tr>
  </tbody>
</table>

<h2>Shell picker</h2>
<p>
  The <strong>▾</strong> dropdown lists every shell that is actually installed and usable on this
  machine, plus any custom terminals you have defined. Shells that aren't found are hidden — you
  won't see <code>zsh</code> on a fresh Windows install, and you won't see <code>cmd</code> on Linux.
</p>
<p>
  Custom terminals are tagged with a <em>custom</em> badge. The footer of the dropdown links straight
  to <strong>Settings → Terminals</strong> for adding more or tweaking detection.
</p>

<h2>Built-in shell catalogue</h2>
<p>Arbor probes for the following shells at startup. Anything missing from <code>PATH</code> is also checked at well-known install locations (e.g. Git Bash under <code>C:\Program Files\Git</code>).</p>
<table class="shortcuts-table">
  <thead><tr><th>Shell</th><th>Default executable</th><th>Platform</th></tr></thead>
  <tbody>
    <tr><td>Command Prompt</td><td><code>cmd.exe</code></td><td>Windows</td></tr>
    <tr><td>Windows PowerShell</td><td><code>powershell.exe</code></td><td>Windows</td></tr>
    <tr><td>PowerShell 7+</td><td><code>pwsh</code></td><td>Any</td></tr>
    <tr><td>Bash</td><td><code>bash</code></td><td>Any</td></tr>
    <tr><td>Git Bash</td><td><code>bash.exe</code> (Git for Windows)</td><td>Windows</td></tr>
    <tr><td>WSL</td><td><code>wsl.exe</code></td><td>Windows</td></tr>
    <tr><td>MSYS2</td><td><code>msys2_shell.cmd</code></td><td>Windows</td></tr>
    <tr><td>Cygwin</td><td><code>Cygwin.bat</code></td><td>Windows</td></tr>
    <tr><td>Zsh</td><td><code>zsh</code></td><td>Linux / macOS</td></tr>
    <tr><td>Fish</td><td><code>fish</code></td><td>Any</td></tr>
    <tr><td>Nushell</td><td><code>nu</code></td><td>Any</td></tr>
    <tr><td>Xonsh</td><td><code>xonsh</code></td><td>Any</td></tr>
    <tr><td>Elvish</td><td><code>elvish</code></td><td>Any</td></tr>
    <tr><td>tcsh</td><td><code>tcsh</code></td><td>Linux / macOS</td></tr>
    <tr><td>sh</td><td><code>sh</code></td><td>Linux / macOS</td></tr>
  </tbody>
</table>
<p>Anything not in this list can still be reached as a <strong>custom terminal</strong> (see <strong>Settings → Terminals</strong>).</p>

<h2>Configuring shells</h2>
<p>Open <strong>Settings → Terminals</strong> (under the <em>Project</em> group) to:</p>
<ul>
  <li>Pick the <strong>default shell</strong> opened by the bare <strong>+</strong> button.</li>
  <li>Override the <strong>executable path</strong> for any built-in shell — useful when the binary isn't on <code>PATH</code> or you want to pin a specific install.</li>
  <li>Add <strong>custom terminals</strong> (any executable + arguments) such as <code>nu --no-config</code>, a containerised dev shell, or a remote SSH helper.</li>
  <li>Re-run <strong>shell detection</strong> after installing a new shell — the picker updates without restarting Arbor.</li>
</ul>
<p>Settings are stored under <code>[terminals]</code> in <code>~/.config/arbor/config.toml</code>.</p>

<h2>Features</h2>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-title">Full colour support</div>
    <div class="fc-desc">256-colour and true-colour ANSI sequences rendered correctly. Works with tools like <code>bat</code>, <code>lazygit</code>, and rich terminal UIs.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Repo-aware tabs</div>
    <div class="fc-desc">Each tab opens in the working directory of the active repository. A small badge in the tab shows the project name.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Auto-close on exit</div>
    <div class="fc-desc">The tab closes automatically ~400 ms after the shell process ends — no need to manually close finished sessions.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Clickable URLs</div>
    <div class="fc-desc">Links in terminal output open in the default browser. Works with <code>http://</code>, <code>https://</code>, and file paths.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">5 000-line scrollback</div>
    <div class="fc-desc">Per-tab scrollback buffer — enough for most build outputs and test runs.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Dynamic resize</div>
    <div class="fc-desc">The terminal reflows automatically when you drag the panel divider. The panel height persists across sessions.</div>
  </div>
</div>

<h2>Resizing the panel</h2>
<p>Drag the divider between the commit graph and the terminal panel to resize. Height is saved in <code>localStorage</code> and restored on next launch.</p>
