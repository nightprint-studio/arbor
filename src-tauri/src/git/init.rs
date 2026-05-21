use std::path::Path;
use git2::{Repository, RepositoryInitOptions, Signature};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

// ---------------------------------------------------------------------------
// Options DTO
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitRepoOptions {
    /// Default branch name (e.g. "main").
    #[serde(default = "default_branch")]
    pub default_branch: String,
    /// Optional description (stored in .git/description and used in README).
    /// Empty string means no description.
    #[serde(default)]
    pub description: String,
    /// Stage all files and create an initial commit after init.
    #[serde(default = "bool_true")]
    pub initial_commit: bool,
    /// Message for the initial commit.
    #[serde(default = "default_commit_msg")]
    pub commit_message: String,
    /// Author name for the initial commit (falls back to git config).
    #[serde(default)]
    pub author_name: String,
    /// Author email for the initial commit (falls back to git config).
    #[serde(default)]
    pub author_email: String,
    /// .gitignore template name (e.g. "Rust", "Node"). Empty/"none" = skip.
    #[serde(default)]
    pub gitignore_template: String,
    /// SPDX license identifier (e.g. "MIT", "Apache-2.0"). Empty/"none" = skip.
    #[serde(default)]
    pub license: String,
    /// Whether to create a README.md.
    #[serde(default)]
    pub readme: bool,
    /// Remote provider: "none" | "github" | "gitlab".
    #[serde(default = "provider_none")]
    pub provider: String,
    /// Repository visibility: "public" | "private".
    #[serde(default = "default_visibility")]
    pub visibility: String,
    /// GitHub org or GitLab group. Empty = personal account.
    #[serde(default)]
    pub org: String,
    /// Explicit remote URL. Non-empty value skips provider API creation.
    #[serde(default)]
    pub remote_url: String,
    /// After a successful init + initial commit, set upstream tracking and
    /// push the initial commit to `origin`. Requires a configured remote.
    #[serde(default)]
    pub push_initial: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitOutcome {
    /// URL of `origin` that was configured (None when `provider == "none"`
    /// and no explicit `remote_url` was provided).
    pub remote_url: Option<String>,
    /// Whether the initial commit was successfully pushed to `origin`.
    /// `false` when push was not requested, no remote was configured, or
    /// the push failed — distinguish via `push_error`.
    pub pushed: bool,
    /// Human-readable push failure message, if the push was attempted and
    /// failed. Missing credentials, permission denied, etc.
    pub push_error: Option<String>,
}

fn default_branch()     -> String { "main".to_string() }
fn default_commit_msg() -> String { "Initial commit".to_string() }
fn default_visibility() -> String { "private".to_string() }
fn provider_none()      -> String { "none".to_string() }
fn bool_true()          -> bool   { true }

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

/// Returns true if `path` is inside a git repository (searches up the tree).
pub fn is_git_repo(path: &str) -> bool {
    Repository::discover(path).is_ok()
}

/// Read user.name and user.email from the global git config.
pub fn get_git_identity() -> (String, String) {
    if let Ok(cfg) = git2::Config::open_default() {
        let name  = cfg.get_string("user.name").unwrap_or_default();
        let email = cfg.get_string("user.email").unwrap_or_default();
        return (name, email);
    }
    (String::new(), String::new())
}

// ---------------------------------------------------------------------------
// Init entry point
// ---------------------------------------------------------------------------

/// Initialise a new git repository at `path`.
/// Returns the init outcome (configured remote + push status).
pub async fn init(path: &str, options: &InitRepoOptions) -> Result<InitOutcome> {
    let p = Path::new(path);

    // git init with the specified default branch name
    let mut init_opts = RepositoryInitOptions::new();
    init_opts.initial_head(&options.default_branch);
    let repo = Repository::init_opts(p, &init_opts)
        .map_err(AppError::Git)?;

    // .git/description
    if !options.description.trim().is_empty() {
        let _ = std::fs::write(p.join(".git").join("description"), options.description.trim());
    }

    // .gitignore
    let tmpl = options.gitignore_template.trim().to_lowercase();
    if !tmpl.is_empty() && tmpl != "none" {
        let content = gitignore_content(&tmpl);
        std::fs::write(p.join(".gitignore"), content)
            .map_err(|e| AppError::Other(format!("write .gitignore: {e}")))?;
    }

    // LICENSE
    let lic = options.license.trim().to_lowercase();
    if !lic.is_empty() && lic != "none" {
        let author = effective_author_name(options);
        let content = license_content(&lic, &author);
        if !content.is_empty() {
            std::fs::write(p.join("LICENSE"), content)
                .map_err(|e| AppError::Other(format!("write LICENSE: {e}")))?;
        }
    }

    // README.md
    if options.readme {
        let name = p.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "project".to_string());
        let desc = options.description.trim();
        let content = if desc.is_empty() {
            format!("# {name}\n")
        } else {
            format!("# {name}\n\n{desc}\n")
        };
        std::fs::write(p.join("README.md"), content)
            .map_err(|e| AppError::Other(format!("write README.md: {e}")))?;
    }

    // Apply pre-resolved remote URL if any (provider creation happens in the
    // command layer so it can talk to the GitProvider registry).
    let remote_url = resolve_remote(options);

    if let Some(url) = &remote_url {
        repo.remote("origin", url).map_err(AppError::Git)?;
    }

    // Initial commit
    let has_commit = if options.initial_commit {
        let (name, email) = effective_identity(options);
        make_initial_commit(&repo, &name, &email, &options.commit_message, &options.default_branch)?;
        true
    } else {
        false
    };

    // Optional: push the initial commit to origin + set upstream tracking.
    // Silently skipped when there's no remote or no commit. Push failures
    // are reported through `InitOutcome.push_error` so the UI can guide the
    // user to fix credentials / retry manually.
    let mut pushed = false;
    let mut push_error: Option<String> = None;
    if options.push_initial && remote_url.is_some() && has_commit {
        let branch = &options.default_branch;
        let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
        match crate::git::remote::push(&repo, "origin", &refspec, false) {
            Ok(_) => {
                // Set upstream tracking so subsequent fetch/pull/push work
                // without -u. Failure here is non-fatal — the push itself
                // succeeded.
                if let Ok(mut cfg) = repo.config() {
                    let _ = cfg.set_str(&format!("branch.{branch}.remote"), "origin");
                    let _ = cfg.set_str(&format!("branch.{branch}.merge"), &format!("refs/heads/{branch}"));
                }
                pushed = true;
            }
            Err(e) => {
                push_error = Some(e.to_string());
            }
        }
    }

    Ok(InitOutcome { remote_url, pushed, push_error })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn effective_author_name(opts: &InitRepoOptions) -> String {
    if !opts.author_name.is_empty() {
        return opts.author_name.clone();
    }
    get_git_identity().0
}

fn effective_identity(opts: &InitRepoOptions) -> (String, String) {
    let (cfg_name, cfg_email) = get_git_identity();
    let name  = if opts.author_name.is_empty()  { cfg_name  } else { opts.author_name.clone()  };
    let email = if opts.author_email.is_empty() { cfg_email } else { opts.author_email.clone() };
    (name, email)
}

fn make_initial_commit(
    repo: &Repository,
    name: &str,
    email: &str,
    message: &str,
    branch: &str,
) -> Result<()> {
    let mut index = repo.index().map_err(AppError::Git)?;
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .map_err(AppError::Git)?;
    index.write().map_err(AppError::Git)?;

    let tree_id = index.write_tree().map_err(AppError::Git)?;
    let tree    = repo.find_tree(tree_id).map_err(AppError::Git)?;

    let author_name  = if name.is_empty()  { "Unknown" } else { name  };
    let author_email = if email.is_empty() { "unknown@localhost" } else { email };

    let sig = Signature::now(author_name, author_email).map_err(AppError::Git)?;

    repo.commit(
        Some(&format!("refs/heads/{branch}")),
        &sig, &sig,
        message,
        &tree,
        &[], // no parents — first commit
    )
    .map_err(AppError::Git)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Remote resolution
// ---------------------------------------------------------------------------
//
// Provider repo creation lives in the command layer (it needs AppState to
// reach the GitProvider registry).  When the caller has already created the
// remote via the provider, it passes the URL through `opts.remote_url` and we
// skip the provider section entirely.

fn resolve_remote(opts: &InitRepoOptions) -> Option<String> {
    let url = opts.remote_url.trim();
    if url.is_empty() { None } else { Some(url.to_string()) }
}

// ---------------------------------------------------------------------------
// .gitignore templates
// ---------------------------------------------------------------------------

pub fn gitignore_content(template: &str) -> String {
    match template {
        "rust" => concat!(
            "# Build artifacts\n",
            "/target/\n",
            "\n",
            "# Cargo.lock — keep for binaries, remove for libraries\n",
            "# Cargo.lock\n",
            "\n",
            "# IDE\n",
            ".idea/\n",
            ".vscode/\n",
            "*.swp\n",
            "*.swo\n",
            "\n",
            "# OS\n",
            ".DS_Store\n",
            "Thumbs.db\n",
        ).to_string(),

        "node" | "nodejs" | "javascript" | "typescript" => concat!(
            "# Dependencies\n",
            "node_modules/\n",
            ".pnp\n",
            ".pnp.js\n",
            "\n",
            "# Build outputs\n",
            "dist/\n",
            "build/\n",
            ".next/\n",
            ".nuxt/\n",
            ".output/\n",
            ".svelte-kit/\n",
            "\n",
            "# Environment\n",
            ".env\n",
            ".env.local\n",
            ".env.development.local\n",
            ".env.test.local\n",
            ".env.production.local\n",
            "\n",
            "# Logs\n",
            "npm-debug.log*\n",
            "yarn-debug.log*\n",
            "yarn-error.log*\n",
            "pnpm-debug.log*\n",
            "\n",
            "# IDE\n",
            ".idea/\n",
            ".vscode/\n",
            "*.swp\n",
            "\n",
            "# OS\n",
            ".DS_Store\n",
            "Thumbs.db\n",
        ).to_string(),

        "python" => concat!(
            "# Bytecode\n",
            "__pycache__/\n",
            "*.py[cod]\n",
            "*$py.class\n",
            "\n",
            "# Virtualenvs\n",
            ".venv/\n",
            "venv/\n",
            "env/\n",
            "ENV/\n",
            "\n",
            "# Distribution\n",
            "dist/\n",
            "build/\n",
            "*.egg-info/\n",
            "\n",
            "# Testing\n",
            ".pytest_cache/\n",
            ".coverage\n",
            "htmlcov/\n",
            "\n",
            "# Tools\n",
            ".mypy_cache/\n",
            ".ruff_cache/\n",
            "\n",
            "# IDE\n",
            ".idea/\n",
            ".vscode/\n",
            "*.swp\n",
            "\n",
            "# OS\n",
            ".DS_Store\n",
            "Thumbs.db\n",
        ).to_string(),

        "go" => concat!(
            "# Binaries\n",
            "*.exe\n",
            "*.exe~\n",
            "*.dll\n",
            "*.so\n",
            "*.dylib\n",
            "\n",
            "# Test binary\n",
            "*.test\n",
            "\n",
            "# Coverage\n",
            "*.out\n",
            "\n",
            "# Go workspace\n",
            "go.work\n",
            "go.work.sum\n",
            "\n",
            "# IDE\n",
            ".idea/\n",
            ".vscode/\n",
            "*.swp\n",
            "\n",
            "# OS\n",
            ".DS_Store\n",
            "Thumbs.db\n",
        ).to_string(),

        "java" => concat!(
            "# Compiled class files\n",
            "*.class\n",
            "\n",
            "# Log files\n",
            "*.log\n",
            "\n",
            "# Package files\n",
            "*.jar\n",
            "*.war\n",
            "*.nar\n",
            "*.ear\n",
            "\n",
            "# Maven\n",
            "target/\n",
            "pom.xml.tag\n",
            "pom.xml.releaseBackup\n",
            "\n",
            "# Gradle\n",
            ".gradle/\n",
            "build/\n",
            "\n",
            "# IDE\n",
            ".idea/\n",
            "*.iml\n",
            ".vscode/\n",
            "*.swp\n",
            "\n",
            "# OS\n",
            ".DS_Store\n",
            "Thumbs.db\n",
        ).to_string(),

        "c" | "c++" | "cpp" => concat!(
            "# Build outputs\n",
            "build/\n",
            "cmake-build-*/\n",
            "*.o\n",
            "*.obj\n",
            "*.a\n",
            "*.lib\n",
            "*.so\n",
            "*.dll\n",
            "*.exe\n",
            "\n",
            "# CMake\n",
            "CMakeCache.txt\n",
            "CMakeFiles/\n",
            "cmake_install.cmake\n",
            "Makefile\n",
            "\n",
            "# IDE\n",
            ".idea/\n",
            ".vscode/\n",
            "*.swp\n",
            "\n",
            "# OS\n",
            ".DS_Store\n",
            "Thumbs.db\n",
        ).to_string(),

        "dotnet" | "csharp" => concat!(
            "# Build results\n",
            "[Dd]ebug/\n",
            "[Rr]elease/\n",
            "[Bb]in/\n",
            "[Oo]bj/\n",
            "[Ll]og/\n",
            "\n",
            "# NuGet\n",
            "*.nupkg\n",
            "*.snupkg\n",
            "**/packages/\n",
            "\n",
            "# IDE\n",
            ".vs/\n",
            ".idea/\n",
            ".vscode/\n",
            "*.suo\n",
            "*.user\n",
            "*.swp\n",
            "\n",
            "# OS\n",
            ".DS_Store\n",
            "Thumbs.db\n",
        ).to_string(),

        "swift" => concat!(
            "# Build\n",
            ".build/\n",
            "xcuserdata/\n",
            "DerivedData/\n",
            "\n",
            "# SPM\n",
            ".swiftpm/\n",
            "\n",
            "# CocoaPods\n",
            "Pods/\n",
            "Podfile.lock\n",
            "\n",
            "# Carthage\n",
            "Carthage/Build/\n",
            "\n",
            "# IDE\n",
            ".idea/\n",
            "*.swp\n",
            "\n",
            "# OS\n",
            ".DS_Store\n",
        ).to_string(),

        "ruby" => concat!(
            "*.gem\n",
            "*.rbc\n",
            ".bundle/\n",
            "vendor/bundle\n",
            "Gemfile.lock\n",
            ".ruby-version\n",
            "\n",
            "# Test / Coverage\n",
            "coverage/\n",
            ".rspec\n",
            "\n",
            "# IDE\n",
            ".idea/\n",
            ".vscode/\n",
            "*.swp\n",
            "\n",
            "# OS\n",
            ".DS_Store\n",
            "Thumbs.db\n",
        ).to_string(),

        "php" => concat!(
            "vendor/\n",
            "node_modules/\n",
            ".env\n",
            ".phpunit.result.cache\n",
            "\n",
            "# IDE\n",
            ".idea/\n",
            ".vscode/\n",
            "*.swp\n",
            "\n",
            "# OS\n",
            ".DS_Store\n",
            "Thumbs.db\n",
        ).to_string(),

        "unity" => concat!(
            "[Ll]ibrary/\n",
            "[Tt]emp/\n",
            "[Oo]bj/\n",
            "[Bb]uild/\n",
            "[Bb]uilds/\n",
            "[Ll]ogs/\n",
            "[Uu]ser[Ss]ettings/\n",
            "*.pidb.meta\n",
            "*.pdb.meta\n",
            "*.mdb.meta\n",
            "\n",
            "# Visual Studio cache directory\n",
            ".vs/\n",
            "\n",
            "# IDE\n",
            ".idea/\n",
            ".vscode/\n",
            "\n",
            "# OS\n",
            ".DS_Store\n",
            "Thumbs.db\n",
        ).to_string(),

        _ => format!("# {template}\n"),
    }
}

// ---------------------------------------------------------------------------
// License templates
// ---------------------------------------------------------------------------

pub fn license_content(license: &str, author: &str) -> String {
    use chrono::Datelike;
    let year = chrono::Local::now().year();

    match license {
        "mit" => format!(
"MIT License

Copyright (c) {year} {author}

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the \"Software\"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"),

        "apache-2.0" => format!(
"                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

Copyright {year} {author}

Licensed under the Apache License, Version 2.0 (the \"License\");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an \"AS IS\" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
"),

        "gpl-3.0" => format!(
"                    GNU GENERAL PUBLIC LICENSE
                       Version 3, 29 June 2007

Copyright (C) {year} {author}

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program. If not, see <https://www.gnu.org/licenses/>.
"),

        "lgpl-3.0" => format!(
"                   GNU LESSER GENERAL PUBLIC LICENSE
                       Version 3, 29 June 2007

Copyright (C) {year} {author}

This library is free software; you can redistribute it and/or modify it
under the terms of the GNU Lesser General Public License as published by
the Free Software Foundation; either version 3 of the License, or
(at your option) any later version.

See <https://www.gnu.org/licenses/>.
"),

        "bsd-2-clause" => format!(
"BSD 2-Clause License

Copyright (c) {year}, {author}

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS \"AS IS\"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
"),

        "bsd-3-clause" => format!(
"BSD 3-Clause License

Copyright (c) {year}, {author}

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors
   may be used to endorse or promote products derived from this software
   without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS \"AS IS\"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
"),

        "isc" => format!(
"ISC License

Copyright (c) {year}, {author}

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED \"AS IS\" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH
REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY
AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT,
INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM
LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR
OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
PERFORMANCE OF THIS SOFTWARE.
"),

        "mpl-2.0" => format!(
"Mozilla Public License Version 2.0

Copyright (c) {year} {author}

This Source Code Form is subject to the terms of the Mozilla Public License,
v. 2.0. If a copy of the MPL was not distributed with this file, You can
obtain one at https://mozilla.org/MPL/2.0/.
"),

        "agpl-3.0" => format!(
"                    GNU AFFERO GENERAL PUBLIC LICENSE
                       Version 3, 19 November 2007

Copyright (C) {year} {author}

This program is free software: you can redistribute it and/or modify it under
the terms of the GNU Affero General Public License as published by the Free
Software Foundation, either version 3 of the License, or (at your option) any
later version.

See <https://www.gnu.org/licenses/>.
"),

        _ => String::new(),
    }
}
