-- vertex-branding/main.lua
--
-- Repaints Arbor with the (fictional) "Vertex Systems" identity for as
-- long as this plugin is loaded. Demonstrates every runtime branding
-- surface in one place:
--
--   • the in-app mark (title-bar slot, welcome screen, About modal,
--     HTML stats export) is swapped via arbor.ui.set_branding{ svg }
--   • the OS-level taskbar / Alt-Tab / window-chrome icon is swapped by
--     passing window_icon_path = <absolute path to a PNG or ICO> to the
--     same call (Tauri's image API needs a rasterised buffer; SVG is not
--     accepted there)
--   • the accent CSS variables are layered on top of the active theme
--     via arbor.ui.set_theme_tokens — overlays survive theme switches
--
-- All overrides are RAM-only: disabling the plugin (or quitting Arbor)
-- restores the bundled identity automatically. Manifest opt-in lives in
-- plugin.toml's [hooks] section: on_plugin_load + on_plugin_unload.

------------------------------------------------------------------------
-- Brand assets
--
-- Both the in-app SVG mark and the OS-level window icon ship as files
-- under assets/. The host reads them off disk when set_branding is
-- called (server-side fs, no fs.read permission required); pass a path
-- via `svg_path` / `window_icon_path` instead of embedding the markup
-- as a Lua long string. The PNG can be regenerated from the SVG with
-- any vector tool (Inkscape, online converter, ImageMagick, …) at
-- ≥ 256×256.
------------------------------------------------------------------------

local SEP       = arbor.meta.os() == "windows" and "\\" or "/"
local ASSETS    = arbor.meta.plugin_dir() .. SEP .. "assets" .. SEP
local SVG_PATH  = ASSETS .. "vertex.svg"
local ICON_PATH = ASSETS .. "vertex.png"

------------------------------------------------------------------------
-- Brand accent palette
--
-- Overlays sit on TOP of whatever theme the user picked: switching from
-- "dark" to "light" still keeps the orange accent. Anything we don't
-- override falls through to the active theme as-is.
------------------------------------------------------------------------

local THEME_OVERLAY = {
  ["--accent"]              = "#f97316",                    -- orange-500
  ["--accent-hover"]        = "#fb923c",                    -- orange-400
  ["--accent-subtle"]       = "rgba(249, 115, 22, 0.16)",   -- 16% orange tint
  ["--border-accent"]       = "rgba(249, 115, 22, 0.42)",   -- focus ring
}

------------------------------------------------------------------------
-- Lifecycle
------------------------------------------------------------------------

arbor.events.on("on_plugin_load", function()
  -- Try the full override first. If the PNG hasn't been generated yet
  -- the backend raises a Lua error — fall back to the SVG-only override
  -- so the user at least sees the in-app mark while they convert the
  -- file. The warning is recorded in the Plugin Logs panel.
  local ok, err = pcall(arbor.ui.set_branding, {
    svg_path         = SVG_PATH,
    window_icon_path = ICON_PATH,
  })
  if not ok then
    arbor.log.warn("vertex-branding: window_icon_path failed (" ..
                   tostring(err) .. ") — applying SVG mark only")
    arbor.ui.set_branding{ svg_path = SVG_PATH }
  end

  arbor.ui.set_theme_tokens{ vars = THEME_OVERLAY }

  arbor.notify{
    title   = "Vertex branding applied",
    message = "Logo, window icon and accent palette switched.",
    level   = "success",
    persist = false,   -- transient toast, no entry in the bell
  }
end)

-- Reload / disable / app-shutdown all fire on_plugin_unload, so the
-- bundled assets come back automatically without extra wiring.
arbor.events.on("on_plugin_unload", function()
  arbor.ui.clear_theme_tokens()
  arbor.ui.clear_branding()
end)
