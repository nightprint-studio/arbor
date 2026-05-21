-- github-actions/main.lua
-- Polls GitHub Actions CI status for commits in the graph.
-- Requires network = ["api.github.com"] in plugin.toml.

-- Cache: oid -> { status, updated_at }
local ci_cache = {}

--- Extract "owner/repo" from a git remote URL.
--- Supports HTTPS (https://github.com/owner/repo.git) and SSH (git@github.com:owner/repo.git).
local function parse_github_slug(url)
  if not url then return nil end
  local owner, repo = url:match("github%.com/([^/]+)/([^/%.]+)")
  if owner and repo then return owner .. "/" .. repo end
  owner, repo = url:match("github%.com:([^/]+)/([^/%.]+)")
  if owner and repo then return owner .. "/" .. repo end
  return nil
end

--- Return the cached CI status for a given commit OID (called from UI layer).
local function get_status(oid)
  return ci_cache[oid] or { status = "unknown" }
end

-- ── Lifecycle ─────────────────────────────────────────────────────────────────

arbor.events.on("on_plugin_load", function(ctx)
  arbor.log.info("CI status overlay ready (api_version=" .. ctx.api_version .. ")")
end)

-- ── Hooks ─────────────────────────────────────────────────────────────────────

arbor.events.on("on_commit", function(ctx)
  arbor.log.info("on_commit: " .. (ctx.oid or "?"))
  -- Invalidate cache for the new commit so it gets refreshed on next fetch.
  if ctx.oid then
    ci_cache[ctx.oid] = nil
  end
end)

arbor.events.on("on_checkout", function(ctx)
  arbor.log.debug("on_checkout branch=" .. (ctx.branch or "?"))
end)

arbor.events.on("on_fetch", function(ctx)
  arbor.log.info("on_fetch remote=" .. (ctx.remote or "origin"))
  -- In a real implementation: batch-request latest commit statuses from
  -- the GitHub Checks API and update ci_cache.
end)
