//! macOS system menu bar — the frontend's hamburger, published to the OS.
//!
//! On Windows and Linux Arbor paints its own chrome and the title-bar hamburger
//! (☰) **is** the menu. On macOS that model reads wrong: the platform expects a
//! real menu bar at the top of the screen, and an app that hides its verbs
//! behind a burger feels like a port. So on the Mac the frontend hides the
//! hamburger and publishes the very same menu here as a serializable tree; the
//! shell turns it into a native [`tauri::menu::Menu`] and emits every click back
//! to the window that published it, where the original handler runs.
//!
//! Ownership: macOS has exactly **one** menu bar per application, so the last
//! window to publish owns it. Each window republishes on focus (frontend side),
//! and clicks route to the publisher — never broadcast — so two product windows
//! can't cross-fire each other's actions.
//!
//! Everything here is a **no-op off macOS**: [`set_native_menu`] returns
//! immediately, so the frontend can call it unconditionally.

use std::sync::Mutex;

use serde::Deserialize;
use tauri::{AppHandle, Emitter};

/// Event the shell emits when a published menu item fires. Payload: the id the
/// frontend registered the item under.
pub const MENU_CLICK_EVENT: &str = "arbor://menu-click";

/// Prefix stamped on every frontend-owned menu id, so a click on a *predefined*
/// item (Quit, Copy, Minimize, …) is never mistaken for one of ours — those
/// carry muda-generated ids and must stay with their native behaviour.
const FE_ID_PREFIX: &str = "fe:";

/// Label of the window that installed the current menu (see module docs).
static OWNER: Mutex<Option<String>> = Mutex::new(None);

// ───────────────────────────────────────────────────────────────────────────
//  Wire format — mirrors `src/lib/ipc/native-menu.ts`
// ───────────────────────────────────────────────────────────────────────────

/// One node of the published tree. Deliberately dumber than the frontend's
/// `DropdownItem`: icons, subtitles, badges and danger styling have no place in
/// a native menu, so the frontend drops them while deriving.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub enum MenuNode {
    Item {
        /// Frontend-side handler key — opaque to the shell, echoed back on click.
        id: String,
        label: String,
        /// Tauri accelerator string (`"CmdOrCtrl+Shift+O"`). An unparsable value
        /// simply yields no accelerator, never an error.
        #[serde(default)]
        accelerator: Option<String>,
        #[serde(default = "default_enabled")]
        enabled: bool,
        /// `Some` renders a checkable row (the frontend's `active` flag).
        #[serde(default)]
        checked: Option<bool>,
    },
    Separator,
    Submenu {
        label: String,
        #[serde(default)]
        items: Vec<MenuNode>,
    },
}

fn default_enabled() -> bool {
    true
}

/// One top-level menu (`File`, `Project`, `Tools`, …).
#[derive(Debug, Deserialize)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub struct MenuGroup {
    pub title: String,
    #[serde(default)]
    pub items: Vec<MenuNode>,
}

/// The whole bar as the publishing window sees it.
#[derive(Debug, Deserialize)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub struct MenuSpec {
    /// Title of the first (application) menu — "Arbor", "Bennu", "Merula", …
    pub app_name: String,
    /// Items that head the application menu — typically the window's own
    /// "About …". When non-empty they REPLACE the system About item (Arbor's
    /// opens an in-app panel); when empty the predefined one is used.
    #[serde(default)]
    pub app_items: Vec<MenuNode>,
    /// The menus that sit between the application menu and `Edit`.
    #[serde(default)]
    pub menus: Vec<MenuGroup>,
}

// ───────────────────────────────────────────────────────────────────────────
//  Command + event
// ───────────────────────────────────────────────────────────────────────────

/// Install `spec` as the application menu bar and route its clicks back to the
/// calling window. No-op off macOS.
///
/// **Async on purpose**: building a menu hops to the main thread and blocks on
/// the reply (`run_main_thread!`), which would deadlock if the command itself
/// ran there — Tauri runs sync commands on the main thread and async ones on the
/// runtime.
#[tauri::command]
pub async fn set_native_menu(
    app: AppHandle,
    window: tauri::WebviewWindow,
    spec: MenuSpec,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(mut owner) = OWNER.lock() {
            *owner = Some(window.label().to_string());
        }
        let menu = mac::build(&app, &spec).map_err(|e| e.to_string())?;
        app.set_menu(menu).map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&app, &window, &spec);
        Ok(())
    }
}

/// Builder-level menu handler: forwards frontend-owned clicks to the publishing
/// window. Predefined items (Quit, Copy, Minimize, …) fall through to their
/// native behaviour untouched.
pub fn on_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    let Some(id) = event.id().0.strip_prefix(FE_ID_PREFIX) else {
        return;
    };
    let Some(label) = OWNER.lock().ok().and_then(|owner| owner.clone()) else {
        return;
    };
    if let Err(e) = app.emit_to(label.as_str(), MENU_CLICK_EVENT, id) {
        tracing::warn!("native menu: click '{id}' not delivered to '{label}': {e}");
    }
}

// ───────────────────────────────────────────────────────────────────────────
//  macOS menu construction
// ───────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod mac {
    use super::{MenuNode, MenuSpec, FE_ID_PREFIX};
    use tauri::menu::{
        AboutMetadata, CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu,
        HELP_SUBMENU_ID, WINDOW_SUBMENU_ID,
    };
    use tauri::{AppHandle, Manager, Wry};

    type Item = Box<dyn IsMenuItem<Wry>>;

    /// Borrow a freshly built list as the `&[&dyn IsMenuItem]` every `with_items`
    /// constructor wants.
    fn refs(items: &[Item]) -> Vec<&dyn IsMenuItem<Wry>> {
        items.iter().map(|i| i.as_ref()).collect()
    }

    /// Assemble the full bar: app menu · published menus · Edit · Window · Help.
    ///
    /// Edit/Window/Help mirror Tauri's own `Menu::default` — on macOS the Edit
    /// menu is load-bearing, not decoration: without it ⌘C/⌘V/⌘Z don't reach text
    /// fields inside the webview at all.
    pub fn build(app: &AppHandle, spec: &MenuSpec) -> tauri::Result<Menu<Wry>> {
        let pkg = app.package_info();
        let config = app.config();
        let about_metadata = AboutMetadata {
            name: Some(spec.app_name.clone()),
            version: Some(pkg.version.to_string()),
            copyright: config.bundle.copyright.clone(),
            authors: config.bundle.publisher.clone().map(|p| vec![p]),
            ..Default::default()
        };

        let app_menu = build_app_menu(app, spec, about_metadata)?;

        let published: Vec<Submenu<Wry>> = spec
            .menus
            .iter()
            .filter(|g| !g.items.is_empty())
            .map(|g| {
                let items = build_nodes(app, &g.items)?;
                Submenu::with_items(app, &g.title, true, &refs(&items))
            })
            .collect::<tauri::Result<_>>()?;

        let edit_menu = Submenu::with_items(
            app,
            "Edit",
            true,
            &[
                &PredefinedMenuItem::undo(app, None)?,
                &PredefinedMenuItem::redo(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::cut(app, None)?,
                &PredefinedMenuItem::copy(app, None)?,
                &PredefinedMenuItem::paste(app, None)?,
                &PredefinedMenuItem::select_all(app, None)?,
            ],
        )?;

        // The ids are how macOS recognises these two as the standard Window and
        // Help menus (window list, the Help search field).
        let window_menu = Submenu::with_id_and_items(
            app,
            WINDOW_SUBMENU_ID,
            "Window",
            true,
            &[
                &PredefinedMenuItem::minimize(app, None)?,
                &PredefinedMenuItem::maximize(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::close_window(app, None)?,
            ],
        )?;
        let help_menu = Submenu::with_id_and_items(app, HELP_SUBMENU_ID, "Help", true, &[])?;

        let mut top: Vec<&dyn IsMenuItem<Wry>> = vec![&app_menu];
        top.extend(published.iter().map(|s| s as &dyn IsMenuItem<Wry>));
        top.push(&edit_menu);
        top.push(&window_menu);
        top.push(&help_menu);

        Menu::with_items(app, &top)
    }

    /// The first menu: the app's own "About …" (or the system one when the
    /// window publishes none) followed by the standard Services/Hide/Quit block.
    fn build_app_menu(
        app: &AppHandle,
        spec: &MenuSpec,
        about_metadata: AboutMetadata<'static>,
    ) -> tauri::Result<Submenu<Wry>> {
        // A window that publishes its own About entry (Arbor's opens an in-app
        // panel, not a system alert) OWNS that slot — adding the predefined item
        // too would list "About Arbor" twice.
        let head = build_nodes(app, &spec.app_items)?;
        let system_about = if head.is_empty() {
            Some(PredefinedMenuItem::about(app, None, Some(about_metadata))?)
        } else {
            None
        };
        let services = PredefinedMenuItem::services(app, None)?;
        let hide = PredefinedMenuItem::hide(app, None)?;
        let hide_others = PredefinedMenuItem::hide_others(app, None)?;
        let quit = PredefinedMenuItem::quit(app, None)?;
        let sep = || PredefinedMenuItem::separator(app);
        let (sep1, sep2, sep3) = (sep()?, sep()?, sep()?);

        let mut children: Vec<&dyn IsMenuItem<Wry>> = Vec::new();
        match &system_about {
            Some(about) => children.push(about),
            None => children.extend(refs(&head)),
        }
        children.push(&sep1);
        children.push(&services);
        children.push(&sep2);
        children.push(&hide);
        children.push(&hide_others);
        children.push(&sep3);
        children.push(&quit);

        Submenu::with_items(app, &spec.app_name, true, &children)
    }

    /// Recursively turn published nodes into native items.
    fn build_nodes(app: &AppHandle, nodes: &[MenuNode]) -> tauri::Result<Vec<Item>> {
        let mut out: Vec<Item> = Vec::with_capacity(nodes.len());
        for node in nodes {
            match node {
                MenuNode::Separator => out.push(Box::new(PredefinedMenuItem::separator(app)?)),
                MenuNode::Item {
                    id,
                    label,
                    accelerator,
                    enabled,
                    checked,
                } => {
                    let id = format!("{FE_ID_PREFIX}{id}");
                    let accel = accelerator.as_deref();
                    match checked {
                        Some(checked) => out.push(Box::new(CheckMenuItem::with_id(
                            app, id, label, *enabled, *checked, accel,
                        )?)),
                        None => out.push(Box::new(MenuItem::with_id(
                            app, id, label, *enabled, accel,
                        )?)),
                    }
                }
                MenuNode::Submenu { label, items } => {
                    let children = build_nodes(app, items)?;
                    out.push(Box::new(Submenu::with_items(
                        app,
                        label,
                        true,
                        &refs(&children),
                    )?));
                }
            }
        }
        Ok(out)
    }
}
