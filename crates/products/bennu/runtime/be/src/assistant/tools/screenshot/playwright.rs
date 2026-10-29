//! The full tier: the project's own Playwright, driven by a small script run through `node`.
//!
//! A project that has Playwright installed has already chosen and installed a browser for it, and it gives
//! what the command-line tier cannot: a screenshot of one element, a full page, waiting for a selector, and —
//! the part that makes "the page is blank" diagnosable — the console errors, the failed requests and the
//! page text.
//!
//! Nothing here is interpolated into code. The request travels as JSON in an environment variable and the
//! script is a constant fed to `node` on stdin, so a selector or a URL cannot become code. The script prints
//! one line, `ARBOR_SHOT:{json}`, which [`parse_report`] reads back; everything else on stdout is ignored.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The package names that provide a `chromium` launcher, best first.
const PACKAGES: [&str; 3] = ["playwright", "@playwright/test", "playwright-core"];

/// The folder whose `node_modules` holds Playwright: `start` or the nearest ancestor that has it. Climbing
/// stops at the filesystem root; a monorepo keeps `node_modules` at its top, not next to the front end.
pub(super) fn find_install(start: &Path) -> Option<PathBuf> {
    start.ancestors().find(|dir| PACKAGES.iter().any(|p| dir.join("node_modules").join(p).join("package.json").is_file())).map(Path::to_path_buf)
}

/// The request the script reads.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct Request<'a> {
    pub url: &'a str,
    pub out: &'a str,
    pub width: u32,
    pub height: u32,
    pub selector: Option<&'a str>,
    pub wait_for: Option<&'a str>,
    pub full_page: bool,
    pub dark: bool,
    /// Take a picture at all; false for diagnostics only.
    pub screenshot: bool,
    /// A system browser to fall back to when Playwright's own was never downloaded.
    pub executable: Option<&'a str>,
    pub timeout_ms: u64,
    pub max_height: u32,
    pub max_bytes: u64,
}

/// What the script reports.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub(super) struct Report {
    pub ok: bool,
    pub error: Option<String>,
    pub status: Option<u16>,
    pub title: String,
    pub text: String,
    pub console: Vec<String>,
    pub page_errors: Vec<String>,
    pub failed_requests: Vec<String>,
    /// `image/png` or `image/jpeg`, when a picture was taken.
    pub media_type: Option<String>,
    /// Where the picture is, when it differs from the requested `out` (a JPEG fallback).
    pub file: Option<String>,
}

/// The last `ARBOR_SHOT:` line of the script's output, decoded.
pub(super) fn parse_report(stdout: &str) -> Result<Report, String> {
    let line = stdout.lines().rev().find_map(|l| l.trim().strip_prefix("ARBOR_SHOT:")).ok_or("the script reported nothing")?;
    serde_json::from_str(line).map_err(|e| format!("the script's report was not readable: {e}"))
}

/// The constant script. CommonJS on purpose: it is read from stdin, where `import` is not available.
pub(super) const SCRIPT: &str = r#"
const req = JSON.parse(process.env.ARBOR_SHOT_REQ);
const fs = require('fs');
const first = (s) => String(s || '').split('\n')[0].slice(0, 300);
function load() {
  for (const name of ['playwright', '@playwright/test', 'playwright-core']) {
    try { return require(require.resolve(name, { paths: [process.cwd()] })); } catch (e) { /* try the next */ }
  }
  throw new Error('Playwright could not be loaded from this project');
}
async function launch(chromium) {
  const attempts = [{}];
  if (req.executable) attempts.push({ executablePath: req.executable });
  let last;
  for (const extra of attempts) {
    try { return await chromium.launch({ headless: true, ...extra }); } catch (e) { last = e; }
  }
  throw new Error('no browser could be started: ' + first(last && last.message));
}
(async () => {
  const report = { ok: false, console: [], page_errors: [], failed_requests: [] };
  let browser;
  try {
    browser = await launch(load().chromium);
    const context = await browser.newContext({ viewport: { width: req.width, height: req.height }, colorScheme: req.dark ? 'dark' : 'light', deviceScaleFactor: 1 });
    const page = await context.newPage();
    const keep = (list, line) => { if (list.length < 25) list.push(line.slice(0, 300)); };
    page.on('console', (m) => { if (m.type() === 'error' || m.type() === 'warning') keep(report.console, m.type() + ': ' + m.text()); });
    page.on('pageerror', (e) => keep(report.page_errors, String((e && e.message) || e)));
    page.on('requestfailed', (r) => keep(report.failed_requests, r.method() + ' ' + r.url() + ' - ' + ((r.failure() && r.failure().errorText) || 'failed')));
    page.on('response', (r) => { if (r.status() >= 400) keep(report.failed_requests, r.request().method() + ' ' + r.url() + ' - HTTP ' + r.status()); });
    const response = await page.goto(req.url, { waitUntil: 'load', timeout: req.timeout_ms });
    report.status = response ? response.status() : null;
    try { await page.waitForLoadState('networkidle', { timeout: 3000 }); } catch (e) { /* a page that never idles is still a page */ }
    if (req.wait_for) {
      if (/^\d+$/.test(req.wait_for)) await page.waitForTimeout(Math.min(Number(req.wait_for), 10000));
      else await page.waitForSelector(req.wait_for, { timeout: Math.min(req.timeout_ms, 20000) });
    }
    report.title = await page.title();
    report.text = ((await page.evaluate(() => (document.body ? document.body.innerText : ''))) || '').replace(/\s+\n/g, '\n').slice(0, 1500);
    if (req.screenshot) {
      const take = async (type, file) => {
        const opts = { path: file, type, ...(type === 'jpeg' ? { quality: 70 } : {}) };
        if (req.selector) {
          const el = page.locator(req.selector).first();
          await el.waitFor({ timeout: 5000 });
          await el.screenshot(opts);
        } else if (req.full_page) {
          const h = await page.evaluate(() => Math.max(document.documentElement.scrollHeight, document.body ? document.body.scrollHeight : 0));
          await page.screenshot({ ...opts, fullPage: true, clip: { x: 0, y: 0, width: req.width, height: Math.max(1, Math.min(h, req.max_height)) } });
        } else {
          await page.screenshot(opts);
        }
      };
      await take('png', req.out);
      report.media_type = 'image/png';
      if (fs.statSync(req.out).size > req.max_bytes) {
        const jpg = req.out.replace(/\.png$/, '') + '.jpg';
        await take('jpeg', jpg);
        report.media_type = 'image/jpeg';
        report.file = jpg;
      }
    }
    report.ok = true;
  } catch (e) {
    report.error = first(e && e.message);
  } finally {
    if (browser) { try { await browser.close(); } catch (e) { /* already gone */ } }
  }
  console.log('ARBOR_SHOT:' + JSON.stringify(report));
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bennu-shot-pw-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn playwright_is_found_in_the_project_or_in_a_folder_above_it() {
        let top = scratch("find");
        let app = top.join("apps").join("web");
        std::fs::create_dir_all(&app).unwrap();
        assert_eq!(find_install(&app), None);
        let pkg = top.join("node_modules").join("@playwright").join("test");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("package.json"), "{}").unwrap();
        assert_eq!(find_install(&app), Some(top.clone()), "a monorepo keeps node_modules at the top");
        let _ = std::fs::remove_dir_all(&top);
    }

    #[test]
    fn a_folder_that_merely_exists_is_not_an_install() {
        let top = scratch("empty");
        std::fs::create_dir_all(top.join("node_modules").join("playwright")).unwrap();
        assert_eq!(find_install(&top), None, "no package.json inside, so not a package");
        let _ = std::fs::remove_dir_all(&top);
    }

    #[test]
    fn the_report_is_the_last_marked_line_whatever_else_was_printed() {
        let out = "debug noise\nARBOR_SHOT:{\"ok\":false}\nmore noise\nARBOR_SHOT:{\"ok\":true,\"title\":\"Shop\",\"status\":200,\"console\":[\"error: boom\"],\"media_type\":\"image/png\"}\n";
        let r = parse_report(out).unwrap();
        assert!(r.ok && r.title == "Shop" && r.status == Some(200));
        assert_eq!(r.console, ["error: boom"]);
        assert_eq!(r.media_type.as_deref(), Some("image/png"));
        assert!(r.page_errors.is_empty(), "absent lists are empty");
    }

    #[test]
    fn a_script_that_printed_nothing_or_garbage_is_an_error() {
        assert!(parse_report("").is_err());
        assert!(parse_report("ARBOR_SHOT:{not json").unwrap_err().contains("not readable"));
    }

    #[test]
    fn the_request_serialises_the_way_the_script_reads_it() {
        let req = Request {
            url: "http://localhost:3000/",
            out: "/tmp/o.png",
            width: 1280,
            height: 800,
            selector: Some("#cart"),
            wait_for: None,
            full_page: false,
            dark: true,
            screenshot: true,
            executable: None,
            timeout_ms: 20_000,
            max_height: 4000,
            max_bytes: 3_000_000,
        };
        let v = serde_json::to_value(&req).unwrap();
        for key in ["url", "out", "width", "height", "selector", "wait_for", "full_page", "dark", "screenshot", "executable", "timeout_ms", "max_height", "max_bytes"] {
            assert!(SCRIPT.contains(&format!("req.{key}")), "the script never reads {key}");
            assert!(v.get(key).is_some(), "{key}");
        }
        assert_eq!(v["selector"], "#cart");
    }

    #[test]
    fn the_script_takes_nothing_from_the_request_as_code() {
        assert!(!SCRIPT.contains("eval(") && !SCRIPT.contains("new Function") && !SCRIPT.contains("page.evaluate(req"));
    }
}
