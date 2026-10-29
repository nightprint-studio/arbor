//! Choosing what to download: the pure half of the samply installer.
//!
//! Release metadata comes from GitHub's `releases/latest` API for `mstange/samply`. The layout this
//! relies on (cargo-dist): one archive per target triple named `samply-<triple>.<ext>`
//! (`.zip` for Windows, `.tar.xz` elsewhere) and, beside each, a `<archive>.sha256` file. The
//! API may also carry a per-asset `digest` (`sha256:<hex>`); when it does, it is preferred.

use serde_json::Value;

pub const LATEST_API_URL: &str = "https://api.github.com/repos/mstange/samply/releases/latest";
/// Every download URL must live under this prefix before any request is made.
const DOWNLOAD_PREFIX: &str = "https://github.com/mstange/samply/releases/download/";

/// Hosts a request or a redirect may touch: the API, the release page, and GitHub's asset CDNs.
const HOSTS: &[&str] = &[
    "api.github.com",
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "github-releases.githubusercontent.com",
];

pub fn host_allowed(scheme: &str, host: Option<&str>) -> bool {
    scheme == "https" && host.is_some_and(|h| HOSTS.contains(&h))
}

/// Whether `url` is a download of a release asset of `mstange/samply`.
pub fn is_release_download_url(url: &str) -> bool {
    url.strip_prefix(DOWNLOAD_PREFIX).is_some_and(|rest| {
        !rest.is_empty()
            && !rest.contains("..")
            && !rest.contains('\\')
            && !rest.contains('?')
            && rest.matches('/').count() == 1
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Zip,
    TarXz,
}

impl ArchiveKind {
    pub fn extension(self) -> &'static str {
        match self {
            ArchiveKind::Zip => "zip",
            ArchiveKind::TarXz => "tar.xz",
        }
    }
}

/// The release archive kind for an OS (`std::env::consts::OS`).
pub fn archive_kind(os: &str) -> ArchiveKind {
    if os == "windows" { ArchiveKind::Zip } else { ArchiveKind::TarXz }
}

/// Target triples to try, best first, for an OS / architecture (`std::env::consts`). Windows on ARM
/// runs the x86-64 build under emulation — the release has no native one.
pub fn target_triples(os: &str, arch: &str) -> &'static [&'static str] {
    match (os, arch) {
        ("windows", "x86_64" | "aarch64") => &["x86_64-pc-windows-msvc"],
        ("macos", "aarch64") => &["aarch64-apple-darwin", "x86_64-apple-darwin"],
        ("macos", "x86_64") => &["x86_64-apple-darwin"],
        ("linux", "x86_64") => &["x86_64-unknown-linux-gnu", "x86_64-unknown-linux-musl"],
        ("linux", "aarch64") => &["aarch64-unknown-linux-gnu"],
        _ => &[],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub size: u64,
    pub url: String,
    /// `sha256:<hex>` as the API reports it, when it does.
    pub digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub version: String,
    pub assets: Vec<ReleaseAsset>,
}

/// `samply-v0.13.1` -> `0.13.1`; a bare `v0.13.1` or `0.13.1` also work.
pub fn version_from_tag(tag: &str) -> String {
    let t = tag.strip_prefix("samply-").unwrap_or(tag);
    t.strip_prefix('v').unwrap_or(t).to_string()
}

pub fn parse_release(json: &Value) -> Result<Release, String> {
    let tag = json["tag_name"].as_str().ok_or("the release has no tag")?.to_string();
    let assets = json["assets"]
        .as_array()
        .ok_or("the release lists no assets")?
        .iter()
        .filter_map(|a| {
            Some(ReleaseAsset {
                name: a["name"].as_str()?.to_string(),
                size: a["size"].as_u64().unwrap_or(0),
                url: a["browser_download_url"].as_str()?.to_string(),
                digest: a["digest"].as_str().map(str::to_string),
            })
        })
        .collect();
    Ok(Release { version: version_from_tag(&tag), tag, assets })
}

/// What to download and how it can be verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    pub asset: ReleaseAsset,
    pub kind: ArchiveKind,
    /// Hex digest from the API, when present.
    pub api_sha256: Option<String>,
    /// The published `<archive>.sha256` file, when the release has one.
    pub checksum_url: Option<String>,
}

pub fn pick_asset(release: &Release, os: &str, arch: &str) -> Option<Pick> {
    let kind = archive_kind(os);
    for triple in target_triples(os, arch) {
        let name = format!("samply-{triple}.{}", kind.extension());
        let Some(asset) = release.assets.iter().find(|a| a.name == name) else { continue };
        if !is_release_download_url(&asset.url) {
            continue;
        }
        let sum_name = format!("{name}.sha256");
        let checksum_url = release
            .assets
            .iter()
            .find(|a| a.name == sum_name && is_release_download_url(&a.url))
            .map(|a| a.url.clone());
        return Some(Pick {
            api_sha256: asset.digest.as_deref().and_then(parse_digest),
            asset: asset.clone(),
            kind,
            checksum_url,
        });
    }
    None
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `sha256:<64 hex>` -> the lowercase hex. Any other algorithm or shape is `None`.
pub fn parse_digest(digest: &str) -> Option<String> {
    let hex = digest.strip_prefix("sha256:")?;
    is_hex64(hex).then(|| hex.to_ascii_lowercase())
}

/// The digest for `asset_name` in a `sha256sum`-style file (`<hex>  <name>`, `<hex> *<name>`), or a
/// bare `<hex>` file. A line that names a different file is not accepted.
pub fn parse_checksum_file(text: &str, asset_name: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hex = parts.next()?;
        if !is_hex64(hex) {
            return None;
        }
        match parts.next() {
            None => Some(hex.to_ascii_lowercase()),
            Some(file) => {
                let file = file.trim_start_matches('*');
                let base = file.rsplit(['/', '\\']).next().unwrap_or(file);
                (base == asset_name).then(|| hex.to_ascii_lowercase())
            }
        }
    })
}

/// Case-insensitive comparison of two hex digests.
pub fn digests_match(expected: &str, actual: &str) -> bool {
    expected.len() == actual.len() && expected.eq_ignore_ascii_case(actual)
}

/// Numeric dotted-version comparison: is `latest` strictly newer than `installed`?
pub fn is_newer(latest: &str, installed: &str) -> bool {
    fn parts(v: &str) -> Vec<u64> {
        v.split(['.', '-', '+']).map_while(|p| p.parse().ok()).collect()
    }
    let (a, b) = (parts(latest), parts(installed));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

/// `samply 0.13.1` (or just `0.13.1`) -> `0.13.1`.
pub fn parse_version_output(out: &str) -> Option<String> {
    let line = out.lines().find(|l| !l.trim().is_empty())?.trim();
    let token = line.split_whitespace().last()?;
    token.chars().next().is_some_and(|c| c.is_ascii_digit()).then(|| token.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn release() -> Release {
        let url = |n: &str| format!("{DOWNLOAD_PREFIX}samply-v0.13.1/{n}");
        let names = [
            "samply-aarch64-apple-darwin.tar.xz",
            "samply-aarch64-apple-darwin.tar.xz.sha256",
            "samply-x86_64-apple-darwin.tar.xz",
            "samply-x86_64-pc-windows-msvc.zip",
            "samply-x86_64-pc-windows-msvc.zip.sha256",
            "samply-x86_64-unknown-linux-gnu.tar.xz",
            "samply-x86_64-unknown-linux-musl.tar.xz",
            "samply-aarch64-unknown-linux-gnu.tar.xz",
            "samply-installer.sh",
        ];
        let assets: Vec<Value> = names
            .iter()
            .map(|n| json!({ "name": n, "size": 1000, "browser_download_url": url(n), "digest": null }))
            .collect();
        parse_release(&json!({ "tag_name": "samply-v0.13.1", "assets": assets })).unwrap()
    }

    #[test]
    fn picks_the_asset_per_os_and_arch() {
        let r = release();
        let name = |os, arch| pick_asset(&r, os, arch).map(|p| p.asset.name);
        assert_eq!(name("windows", "x86_64").as_deref(), Some("samply-x86_64-pc-windows-msvc.zip"));
        assert_eq!(name("windows", "aarch64").as_deref(), Some("samply-x86_64-pc-windows-msvc.zip"));
        assert_eq!(name("macos", "aarch64").as_deref(), Some("samply-aarch64-apple-darwin.tar.xz"));
        assert_eq!(name("macos", "x86_64").as_deref(), Some("samply-x86_64-apple-darwin.tar.xz"));
        assert_eq!(name("linux", "x86_64").as_deref(), Some("samply-x86_64-unknown-linux-gnu.tar.xz"));
        assert_eq!(name("linux", "aarch64").as_deref(), Some("samply-aarch64-unknown-linux-gnu.tar.xz"));
        assert_eq!(name("freebsd", "x86_64"), None);
        assert_eq!(name("linux", "riscv64"), None);
    }

    #[test]
    fn linux_falls_back_to_musl_and_reports_the_checksum_file() {
        let mut r = release();
        r.assets.retain(|a| !a.name.contains("linux-gnu"));
        let p = pick_asset(&r, "linux", "x86_64").unwrap();
        assert_eq!(p.asset.name, "samply-x86_64-unknown-linux-musl.tar.xz");
        assert_eq!(p.checksum_url, None);
        let w = pick_asset(&r, "windows", "x86_64").unwrap();
        assert!(w.checksum_url.unwrap().ends_with("pc-windows-msvc.zip.sha256"));
        assert_eq!(w.kind, ArchiveKind::Zip);
    }

    #[test]
    fn an_asset_url_outside_the_repo_is_never_picked() {
        let mut r = release();
        for a in &mut r.assets {
            a.url = format!("https://evil.example/{}", a.name);
        }
        assert_eq!(pick_asset(&r, "windows", "x86_64"), None);
    }

    #[test]
    fn download_urls_are_pinned_to_the_repo() {
        assert!(is_release_download_url("https://github.com/mstange/samply/releases/download/samply-v0.13.1/samply-x.zip"));
        for bad in [
            "http://github.com/mstange/samply/releases/download/t/f.zip",
            "https://github.com/other/samply/releases/download/t/f.zip",
            "https://github.com/mstange/samply/releases/download/../x/f.zip",
            "https://github.com/mstange/samply/releases/download/t/f.zip?x=1",
            "https://github.com/mstange/samply/releases/download/t",
            "https://github.com.evil.example/mstange/samply/releases/download/t/f.zip",
        ] {
            assert!(!is_release_download_url(bad), "{bad}");
        }
    }

    #[test]
    fn hosts_need_https_and_the_allow_list() {
        assert!(host_allowed("https", Some("objects.githubusercontent.com")));
        assert!(!host_allowed("http", Some("github.com")));
        assert!(!host_allowed("https", Some("github.com.evil.example")));
        assert!(!host_allowed("https", None));
    }

    #[test]
    fn digests_parse_only_sha256() {
        assert_eq!(parse_digest(&format!("sha256:{}", HEX.to_uppercase())).as_deref(), Some(HEX));
        assert_eq!(parse_digest(&format!("sha512:{HEX}")), None);
        assert_eq!(parse_digest("sha256:abc"), None);
    }

    #[test]
    fn checksum_files_must_name_the_asset() {
        let other = "f".repeat(64);
        let text = format!("{other}  other.zip\n{HEX} *samply-x.zip\n");
        assert_eq!(parse_checksum_file(&text, "samply-x.zip").as_deref(), Some(HEX));
        assert_eq!(parse_checksum_file(&text, "missing.zip"), None);
        assert_eq!(parse_checksum_file(&format!("{HEX}\n"), "anything").as_deref(), Some(HEX));
        assert_eq!(parse_checksum_file("not a checksum  samply-x.zip", "samply-x.zip"), None);
        assert_eq!(parse_checksum_file(&format!("{HEX}  dist/samply-x.zip"), "samply-x.zip").as_deref(), Some(HEX));
    }

    #[test]
    fn digest_comparison_ignores_case_and_rejects_prefixes() {
        assert!(digests_match(HEX, &HEX.to_uppercase()));
        assert!(!digests_match(HEX, &HEX[..63]));
        assert!(!digests_match(HEX, &"0".repeat(64)));
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("0.13.1", "0.13.0"));
        assert!(is_newer("0.14.0", "0.9.9"));
        assert!(is_newer("1.0", "0.99.99"));
        assert!(!is_newer("0.13.1", "0.13.1"));
        assert!(!is_newer("0.13.0", "0.13.1"));
        assert_eq!(version_from_tag("samply-v0.13.1"), "0.13.1");
        assert_eq!(parse_version_output("samply 0.13.1\n").as_deref(), Some("0.13.1"));
        assert_eq!(parse_version_output("error: nope"), None);
    }
}
