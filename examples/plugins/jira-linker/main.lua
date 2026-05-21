-- jira-linker/main.lua
-- Scans commit messages for JIRA-style issue keys (e.g. PROJ-1234).
-- Fully offline — no network permissions required.

-- Pattern matching JIRA issue keys: one or more uppercase letters, dash, digits.
local JIRA_PATTERN = "([A-Z][A-Z0-9]+%-[0-9]+)"

-- In-memory index: oid → list of issue keys.
local commit_issues = {}

-- Configurable JIRA base URL (opens browser when user clicks a key).
local jira_base_url = ""

--- Extract all JIRA keys from a string, deduplicated.
local function extract_keys(text)
  local keys, seen = {}, {}
  for key in text:gmatch(JIRA_PATTERN) do
    if not seen[key] then
      seen[key] = true
      keys[#keys + 1] = key
    end
  end
  return keys
end

-- ── Lifecycle ─────────────────────────────────────────────────────────────────

arbor.events.on("on_plugin_load", function(ctx)
  jira_base_url = arbor.settings.global.get("jira_url") or ""
  arbor.log.info("JIRA key detection active (api_version=" .. ctx.api_version .. ")")
end)

-- ── Hooks ─────────────────────────────────────────────────────────────────────

arbor.events.on("on_commit", function(ctx)
  local msg  = ctx.message or ""
  local oid  = ctx.oid or ""
  local keys = extract_keys(msg)
  if #keys > 0 then
    commit_issues[oid] = keys
    arbor.log.info("commit " .. oid:sub(1, 8) .. " references: " ..
      table.concat(keys, ", "))
  end
end)

-- ── Settings ──────────────────────────────────────────────────────────────────

arbor.ui.add_context_menu_item({
  target = "commit",
  label  = "Show JIRA Issues",
  action = "jira:show_issues",
  icon   = "Link",
})

arbor.events.on("jira:show_issues", function(ctx)
  local keys = commit_issues[ctx.oid or ""] or {}
  if #keys == 0 then
    arbor.notify{ message = "No JIRA keys found in this commit.", level = "info" }
    return
  end
  arbor.notify{ message = "Issues: " .. table.concat(keys, ", "), level = "info" }
end)

arbor.events.on("jira:configure", function(_ctx)
  arbor.ui.form({
    title         = "JIRA Linker Settings",
    submit_label  = "Save",
    submit_action = "jira:save_config",
    nodes = {
      { type = "text", name = "jira_url", label = "JIRA Base URL",
        placeholder = "https://yourcompany.atlassian.net",
        default = arbor.settings.global.get("jira_url") or "",
        hint = "Used to build links when clicking issue keys." },
    },
  })
end)

arbor.events.on("jira:save_config", function(ctx)
  arbor.settings.global.set("jira_url", ctx.jira_url or "")
  jira_base_url = ctx.jira_url or ""
  arbor.notify{ message = "JIRA Linker settings saved.", level = "success" }
end)
