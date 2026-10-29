//! The assistant's eyes on a front end: a screenshot of a page on this machine, with what the browser
//! complained about while loading it.
//!
//! Three tiers, tried in this order, and the answer says which one it got:
//!
//! 1. **The project's own Playwright** (`node_modules/playwright` or `@playwright/test`), driven by a small
//!    constant script through `node`: the screenshot, one element or the full page, waiting for a selector,
//!    and the console errors, failed requests and page text. (`playwright.rs`)
//! 2. **A Chromium-based browser already installed** (Chrome, Edge, Brave, Chromium), asked for
//!    `--headless --screenshot`: the picture only. (`browser.rs`, `chrome.rs`)
//! 3. **Neither**: a message naming the two ways to get one.
//!
//! Nothing is downloaded or added to the project. A page is only ever on this machine (`target.rs`); the
//! picture is stored under the assistant's data folder and reaches the model as an image block inside the
//! tool's result (`store.rs`), where only the latest few are kept — see `bennu_assistant::images`.
//!
//! ## What asks the user
//!
//! Nothing. It reads a page on localhost the way `http_request` reads with GET, and writes a file only in a
//! temporary folder that is removed afterwards.

mod browser;
mod chrome;
mod playwright;
mod run;
mod store;
mod target;

use std::path::{Path, PathBuf};
use std::time::Duration;

use bennu_assistant::prelude::{estimate_image_tokens, image_dimensions, ToolSpec, ImageRef};
use serde_json::{json, Value};

use super::{clip, spec, ToolHost};
use crate::live::live_for;
pub(in crate::assistant) use store::prepare_messages;

pub(super) const SCREENSHOT: &str = "screenshot_page";
pub(super) const DIAGNOSTICS: &str = "page_diagnostics";

pub(super) fn is_screenshot_tool(name: &str) -> bool {
    name == SCREENSHOT || name == DIAGNOSTICS
}

/// The picture is refused above this size: the provider takes about 5 MB of base64, which is 3.75 MB of file.
const MAX_IMAGE_BYTES: u64 = 3_500_000;
/// How long a browser or `node` may take over one capture.
const PLAYWRIGHT_LIMIT: Duration = Duration::from_secs(60);
const CHROME_LIMIT: Duration = Duration::from_secs(40);
const NAVIGATION_TIMEOUT_MS: u64 = 20_000;

pub(super) fn specs() -> Vec<ToolSpec> {
    let page = |extra: Value| {
        let mut props = json!({
            "url":{"type":"string","description":"http://localhost:PORT/path — this machine only (localhost, 127.0.0.1, [::1])."},
            "port":{"type":"integer","description":"Instead of `url`: the port of a process you started (run_start / run_recipe). `path` goes with it."},
            "path":{"type":"string","description":"With `port`: the page, e.g. `/login`. Default `/`."},
            "wait_for":{"type":"string","description":"A CSS selector to wait for, or a number of milliseconds to let the page settle. Default: the page's own load, then a short idle."}
        });
        if let (Some(p), Some(e)) = (props.as_object_mut(), extra.as_object()) {
            p.extend(e.clone());
        }
        json!({"type":"object","properties":props})
    };
    vec![
        spec(
            SCREENSHOT,
            "Look at a page of a web front end running on THIS machine: returns a screenshot as an image, plus — when the project has Playwright — the page title, the console errors, the failed network requests and a snippet of the visible text. Use it to check what a UI change looks like, or why a page is blank. Start the app first (run_start / run_recipe, then wait_ready). Images are expensive: the conversation keeps only the latest two, so take a screenshot to answer a question, not to poll. Without Playwright in the project, a Chromium-based browser installed on the machine is used and only the picture is returned (no selector, full page or diagnostics).",
            page(json!({
                "selector":{"type":"string","description":"Capture only this element (CSS selector). Needs Playwright."},
                "viewport":{"type":"object","properties":{"w":{"type":"integer"},"h":{"type":"integer"}},"description":"Window size in pixels. Default 1280 x 800; 390 x 844 is a phone."},
                "full_page":{"type":"boolean","description":"The whole scrollable page, up to 4000 px tall. Needs Playwright."},
                "dark":{"type":"boolean","description":"Ask for the dark colour scheme (prefers-color-scheme)."}
            })),
        ),
        spec(
            DIAGNOSTICS,
            "Load a page on THIS machine and report, as text and with no image (cheap): its title, HTTP status, console errors and warnings, uncaught page errors, failed or 4xx/5xx network requests, and a snippet of the visible text. The first thing to run when a page is blank or broken. Needs Playwright in the project (node_modules/playwright or @playwright/test).",
            page(json!({"viewport":{"type":"object","properties":{"w":{"type":"integer"},"h":{"type":"integer"}}}})),
        ),
    ]
}

// ── Pure text ────────────────────────────────────────────────────────────────────────────────────

/// What the browser complained about, one section per kind, empty when there was nothing to say.
fn diagnostics_text(r: &playwright::Report) -> String {
    let mut out = String::new();
    let status = r.status.map(|s| format!(", HTTP {s}")).unwrap_or_default();
    out.push_str(&format!("page: \"{}\"{status}\n", r.title.trim()));
    let section = |label: &str, lines: &[String]| -> String {
        if lines.is_empty() {
            return String::new();
        }
        format!("{label} ({}):\n{}\n", lines.len(), lines.iter().map(|l| format!("  {l}")).collect::<Vec<_>>().join("\n"))
    };
    out.push_str(&section("uncaught errors", &r.page_errors));
    out.push_str(&section("console errors and warnings", &r.console));
    out.push_str(&section("failed requests", &r.failed_requests));
    if r.page_errors.is_empty() && r.console.is_empty() && r.failed_requests.is_empty() {
        out.push_str("no console errors, uncaught errors or failed requests.\n");
    }
    let text = r.text.trim();
    if text.is_empty() {
        out.push_str("the page shows no text (blank, or drawn on a canvas).\n");
    } else {
        out.push_str(&format!("visible text: {}\n", text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(600).collect::<String>()));
    }
    out
}

/// What the command-line tier could not honour, so the model does not believe it got it.
fn ignored_options(selector: bool, full_page: bool, wait_selector: bool) -> Option<String> {
    let mut left_out = Vec::new();
    if selector {
        left_out.push("`selector` (the whole viewport was captured)");
    }
    if full_page {
        left_out.push("`full_page` (only the viewport was captured)");
    }
    if wait_selector {
        left_out.push("waiting for a selector (the page was given a few seconds to settle instead)");
    }
    (!left_out.is_empty()).then(|| format!("not available without Playwright in the project: {}.", left_out.join(", ")))
}

const NO_BROWSER: &str = "No way to take a screenshot here: the project has no Playwright and no Chrome, Edge, Brave or Chromium was found on this machine. Either install one of those browsers, or add Playwright to the project (`npm i -D playwright` then `npx playwright install chromium`) — it also gives the console errors and failed requests. CHROME_PATH can point at a browser in an unusual place.";

const NEEDS_PLAYWRIGHT: &str = "page_diagnostics needs Playwright in the project (node_modules/playwright or @playwright/test) and `node` on the PATH — nothing else can report a page's console. screenshot_page still works with an installed Chrome, Edge, Brave or Chromium, picture only.";

/// Whether a Playwright failure means "this tier cannot run here" (so the next one is tried) rather than
/// "the page itself failed" (which the model needs to hear about).
fn tier_unavailable(error: &str) -> bool {
    error.contains("could not be loaded") || error.contains("no browser could be started")
}

// ── The tool ─────────────────────────────────────────────────────────────────────────────────────

/// A scratch folder in the system temp dir, removed when dropped — the browser's profile and its output.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self, String> {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("arbor-bennu-shot-{}-{nanos:x}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| format!("could not make a temporary folder: {e}"))?;
        Ok(Scratch(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// What a tier produced.
struct Capture {
    /// The picture, when one was asked for.
    image: Option<(Vec<u8>, &'static str)>,
    report: Option<playwright::Report>,
    /// The size asked for, used when the file's own header cannot be read.
    asked: (u32, u32),
    tier: String,
    ignored: Option<String>,
}

/// Everything a capture needs, read from the model's arguments.
struct Options<'a> {
    target: &'a target::Target,
    width: u32,
    height: u32,
    selector: Option<&'a str>,
    wait_for: Option<&'a str>,
    full_page: bool,
    dark: bool,
    picture: bool,
}

fn text_arg<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

/// Something is accepting connections on `port` of this machine.
fn listening(port: u16) -> bool {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
    [IpAddr::V4(Ipv4Addr::LOCALHOST), IpAddr::V6(Ipv6Addr::LOCALHOST)]
        .into_iter()
        .any(|ip| TcpStream::connect_timeout(&SocketAddr::new(ip, port), Duration::from_millis(1500)).is_ok())
}

impl ToolHost {
    /// The ports declared by processes that are still up — the ones `port:` may name.
    fn managed_ports(&self) -> Vec<u16> {
        live_for(&self.session.root, &self.session.sink).manager.list().into_iter().filter(|p| !p.phase.is_finished()).flat_map(|p| p.ports).collect()
    }

    pub(super) fn run_screenshot(&self, call_id: &str, tool: &str, input: &Value) -> Result<String, String> {
        let target = target::resolve(input, &self.managed_ports())?;
        if !listening(target.port) {
            return Err(format!("nothing is listening on port {}. Start the app (run_start / run_recipe) and wait_ready, then try again.", target.port));
        }
        let (width, height) = target::viewport(input);
        let picture = tool == SCREENSHOT;
        let options = Options {
            target: &target,
            width,
            height,
            selector: text_arg(input, "selector"),
            wait_for: text_arg(input, "wait_for"),
            full_page: input.get("full_page").and_then(Value::as_bool).unwrap_or(false),
            dark: input.get("dark").and_then(Value::as_bool).unwrap_or(false),
            picture,
        };
        let scratch = Scratch::new()?;
        let capture = self.capture(&options, &scratch.0)?;

        let mut text = String::new();
        if let Some((bytes, media)) = &capture.image {
            if bytes.len() as u64 > MAX_IMAGE_BYTES {
                return Err(format!("the screenshot is {} MB, more than the model can be sent. Ask for a smaller `viewport`, a `selector`, or no `full_page`.", bytes.len() / 1_000_000));
            }
            let (w, h) = image_dimensions(bytes).unwrap_or(capture.asked);
            let path = store::save(&self.session.conversation_id, bytes, media)?;
            let tokens = estimate_image_tokens(w, h);
            store::hold(
                call_id,
                store::Pending {
                    image: ImageRef { media_type: (*media).to_string(), path, tokens },
                    width: w,
                    height: h,
                    caption: format!("{} · {w}×{h}", target.url),
                },
            );
            text.push_str(&format!("screenshot of {} ({w}x{h}, about {tokens} tokens) is attached to this result. Captured with {}.\n", target.url, capture.tier));
        } else {
            text.push_str(&format!("{} — diagnostics from {}.\n", target.url, capture.tier));
        }
        if let Some(note) = &capture.ignored {
            text.push_str(&format!("{note}\n"));
        }
        if let Some(report) = &capture.report {
            text.push_str(&diagnostics_text(report));
        }
        Ok(clip(text))
    }

    /// Choose the tier and run it. `Err` is the model-facing reason nothing could be captured.
    fn capture(&self, o: &Options, scratch: &Path) -> Result<Capture, String> {
        let node = arbor_process_ext::prelude::locate_executable("node", None, &[], &[]);
        let install = playwright::find_install(self.project());
        let mut why_not_playwright = None;
        if let (Some(node), Some(dir)) = (node, install) {
            match self.with_playwright(o, scratch, &node, &dir) {
                Ok(c) => return Ok(c),
                Err(e) if tier_unavailable(&e) && o.picture => why_not_playwright = Some(e),
                Err(e) => return Err(e),
            }
        }
        if !o.picture {
            return Err(NEEDS_PLAYWRIGHT.to_string());
        }
        let mut capture = self.with_browser(o, scratch)?;
        if let Some(why) = why_not_playwright {
            capture.ignored = Some(format!("Playwright is in the project but could not be used ({why}); fell back to the installed browser, picture only."));
        }
        Ok(capture)
    }

    fn with_playwright(&self, o: &Options, scratch: &Path, node: &str, dir: &Path) -> Result<Capture, String> {
        let out = scratch.join("page.png");
        let (out_text, executable) = (out.display().to_string(), browser::find().map(|b| b.path.display().to_string()));
        let request = playwright::Request {
            url: &o.target.url,
            out: &out_text,
            width: o.width,
            height: o.height,
            selector: o.selector,
            wait_for: o.wait_for,
            full_page: o.full_page,
            dark: o.dark,
            screenshot: o.picture,
            executable: executable.as_deref(),
            timeout_ms: NAVIGATION_TIMEOUT_MS,
            max_height: target::MAX_FULL_PAGE_HEIGHT,
            max_bytes: MAX_IMAGE_BYTES,
        };
        let mut cmd = std::process::Command::new(node);
        cmd.arg("-").current_dir(dir).env("ARBOR_SHOT_REQ", serde_json::to_string(&request).map_err(|e| e.to_string())?);
        let done = run::run(cmd, Some(playwright::SCRIPT), PLAYWRIGHT_LIMIT, &|| self.session.cancelled()).map_err(|s| s.message("the Playwright script", PLAYWRIGHT_LIMIT))?;
        let report = playwright::parse_report(&done.stdout).map_err(|e| format!("{e}. {}", last_lines(&done.stderr)))?;
        if !report.ok {
            let error = report.error.clone().unwrap_or_else(|| "unknown error".into());
            return Err(if tier_unavailable(&error) { error } else { format!("the page could not be captured: {error}. If the app only just started, wait_ready and try again; run_logs shows what it printed.") });
        }
        let image = match (o.picture, &report.media_type) {
            (true, Some(media)) => {
                let file = report.file.as_deref().map(PathBuf::from).unwrap_or(out);
                let bytes = std::fs::read(&file).map_err(|e| format!("the screenshot was not written: {e}"))?;
                Some((bytes, if media == "image/jpeg" { "image/jpeg" } else { "image/png" }))
            }
            (true, None) => return Err("Playwright finished without a picture".into()),
            _ => None,
        };
        Ok(Capture { image, report: Some(report), asked: (o.width, o.height), tier: "the project's Playwright".into(), ignored: None })
    }

    fn with_browser(&self, o: &Options, scratch: &Path) -> Result<Capture, String> {
        let browser = browser::find().ok_or_else(|| NO_BROWSER.to_string())?;
        let out = scratch.join("page.png");
        let profile = scratch.join("profile");
        let settle = chrome::settle_ms(o.wait_for);
        let shot = chrome::Shot { url: &o.target.url, out: &out, profile: &profile, width: o.width, height: o.height, dark: o.dark, settle_ms: settle };
        let mut cmd = std::process::Command::new(&browser.path);
        cmd.args(chrome::args(&shot));
        let done = run::run(cmd, None, CHROME_LIMIT, &|| self.session.cancelled()).map_err(|s| s.message("the browser", CHROME_LIMIT))?;
        let bytes = std::fs::read(&out).map_err(|_| format!("the browser ended without writing a screenshot ({}). {}", if done.ok { "it exited normally" } else { "it exited with an error" }, last_lines(&done.stderr)))?;
        let name = browser.path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "a browser".into());
        let wait_selector = o.wait_for.is_some_and(|w| w.parse::<u64>().is_err());
        Ok(Capture {
            image: Some((bytes, "image/png")),
            report: None,
            asked: (o.width, o.height),
            tier: format!("{name} (headless, picture only)"),
            ignored: ignored_options(o.selector.is_some(), o.full_page, wait_selector),
        })
    }

    /// The pictures the call that just ran left behind, drawn in the thread and handed to the loop.
    pub(super) fn shown_images(&self, call_id: &str) -> Vec<ImageRef> {
        let held = store::take(call_id);
        if held.is_empty() {
            return Vec::new();
        }
        for shot in &held {
            self.session.view().push(store::view_block(call_id, shot));
        }
        self.session.push_message();
        held.into_iter().map(|s| s.image).collect()
    }
}

/// The last few lines of a stream, for a failure that needs more than "it failed".
fn last_lines(text: &str) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = lines[lines.len().saturating_sub(4)..].join(" | ");
    if tail.is_empty() { String::new() } else { format!("Its output ended with: {}", tail.chars().take(500).collect::<String>()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_tools_are_specs_and_only_they_are_this_family() {
        let names: Vec<String> = specs().into_iter().map(|s| s.name).collect();
        assert_eq!(names, [SCREENSHOT, DIAGNOSTICS]);
        assert!(is_screenshot_tool("screenshot_page") && is_screenshot_tool("page_diagnostics") && !is_screenshot_tool("http_request"));
    }

    #[test]
    fn the_description_warns_that_pictures_are_expensive_and_that_the_page_is_local() {
        let d = &specs()[0].description;
        assert!(d.contains("expensive") && d.contains("THIS machine") && d.contains("latest two"));
    }

    fn report() -> playwright::Report {
        playwright::Report {
            ok: true,
            title: "Shop".into(),
            status: Some(200),
            text: "Cart\n\n  3 items".into(),
            console: vec!["error: Failed to load resource".into()],
            page_errors: vec!["TypeError: x is undefined".into()],
            failed_requests: vec!["GET http://localhost:8080/api/cart - HTTP 500".into()],
            ..Default::default()
        }
    }

    #[test]
    fn the_diagnostics_list_each_kind_of_complaint_and_the_text() {
        let t = diagnostics_text(&report());
        assert!(t.contains("page: \"Shop\", HTTP 200"), "{t}");
        assert!(t.contains("uncaught errors (1)") && t.contains("console errors and warnings (1)") && t.contains("failed requests (1)"), "{t}");
        assert!(t.contains("visible text: Cart 3 items"), "{t}");
    }

    #[test]
    fn a_clean_page_says_so_and_a_blank_one_says_that_too() {
        let clean = playwright::Report { ok: true, title: "T".into(), text: String::new(), ..Default::default() };
        let t = diagnostics_text(&clean);
        assert!(t.contains("no console errors") && t.contains("shows no text"), "{t}");
    }

    #[test]
    fn what_the_command_line_tier_dropped_is_named() {
        assert_eq!(ignored_options(false, false, false), None);
        let note = ignored_options(true, true, true).unwrap();
        assert!(note.contains("selector") && note.contains("full_page") && note.contains("waiting for a selector"), "{note}");
    }

    #[test]
    fn only_a_missing_tier_falls_through_not_a_failed_page() {
        assert!(tier_unavailable("Playwright could not be loaded from this project"));
        assert!(tier_unavailable("no browser could be started: Executable doesn't exist"));
        assert!(!tier_unavailable("page.goto: net::ERR_CONNECTION_REFUSED"));
    }

    #[test]
    fn a_failure_shows_the_end_of_its_output() {
        assert_eq!(last_lines(""), "");
        let t = last_lines("a\n\nb\nc\nd\ne\n");
        assert!(t.contains("b | c | d | e") && !t.contains("a |"), "{t}");
    }

    #[test]
    fn the_scratch_folder_goes_away_with_its_owner() {
        let path = {
            let s = Scratch::new().unwrap();
            std::fs::write(s.0.join("page.png"), b"x").unwrap();
            assert!(s.0.is_dir());
            s.0.clone()
        };
        assert!(!path.exists());
    }

    #[test]
    fn a_closed_port_is_not_listening() {
        // Port 1 is reserved and nothing listens on it.
        assert!(!listening(1));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        assert!(listening(listener.local_addr().unwrap().port()));
    }
}
