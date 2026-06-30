-- arbor.core.assert — validation gates for pipelines.
--
-- Each op fails the step with exit=1 when its condition isn't satisfied,
-- short-circuiting the stage unless the step sets allow_failure=true.
--
-- Ops
--   · assert_file_exists       { path, negate? }
--   · assert_file_not_contains { path, pattern, negate? }
--   · assert_glob_matches      { glob, min?, max? }
--   · assert_version_bump      { file, new_version }   -- pom.xml|package.json|Cargo.toml

local U = require("arbor.core._util")

local M = {}

function M.assert_file_exists(params, ctx)
  local path   = U.abs_path(params.path or "", ctx)
  local negate = params.negate and true or false
  local lines, add = U.new_log(negate and "assert_file_exists NOT" or "assert_file_exists")
  U.log_entry(add, "check ", path)
  U.log_entry(add, "expect", negate and "must NOT exist" or "must exist")
  local exists = arbor.fs.exists(path)
  local pass = (exists and not negate) or (not exists and negate)
  if pass then
    add("PASS")
    return U.finish(lines)
  end
  return U.fail(lines, "expected " .. (negate and "absent" or "present") .. ": " .. path)
end

function M.assert_file_not_contains(params, ctx)
  local path    = U.abs_path(params.path or "", ctx)
  local pattern = params.pattern or ""
  local negate  = params.negate and true or false
  -- Default semantics: must NOT contain. `negate=true` flips to "MUST contain".
  local lines, add = U.new_log(negate and "assert_file_not_contains NOT" or "assert_file_not_contains")
  U.log_entry(add, "file   ", path)
  U.log_entry(add, "pattern", pattern)
  U.log_entry(add, "expect ", negate and "pattern MUST be present" or "pattern must NOT be present")
  if not arbor.fs.exists(path) then
    return U.fail(lines, "file not found: " .. path)
  end
  local content = arbor.fs.read(path) or ""
  local contains, err = arbor.text.contains{ content = content, pattern = pattern, plain = false }
  if err then
    return U.fail(lines, "regex error: " .. tostring(err))
  end
  local pass = (contains and negate) or (not contains and not negate)
  U.log_entry(add, "found  ", contains)
  if pass then
    add("PASS")
    return U.finish(lines)
  end
  return U.fail(lines, "pattern condition not satisfied")
end

function M.assert_glob_matches(params, ctx)
  local glob = params.glob or ""
  local min  = tonumber(params.min) or 1
  local max  = tonumber(params.max)
  local root = (ctx and ctx.cwd) or ""
  local lines, add = U.new_log("assert_glob_matches")
  U.log_entry(add, "root", root)
  U.log_entry(add, "glob", glob)
  U.log_entry(add, "min ", min)
  U.log_entry(add, "max ", max or "unlimited")
  local basename = glob:match("[^/\\]+$") or glob
  -- include_dirs=true so assertions like "exactly one exploded webapp dir"
  -- can be expressed with a glob like `app/*` + max=1.
  local hits = arbor.fs.glob{ root = root, pattern = basename, include_dirs = true } or {}
  local n = #hits
  U.log_entry(add, "found", n)
  if n < min then
    return U.fail(lines, "expected >= " .. min .. ", got " .. n)
  end
  if max and n > max then
    return U.fail(lines, "expected <= " .. max .. ", got " .. n)
  end
  add("PASS")
  return U.finish(lines)
end

-- Semver comparison (major.minor.patch). Prerelease tags after `-` are
-- ignored. Returns negative if a<b, zero if equal, positive if a>b.
local function semver_cmp(a, b)
  local function parts(s)
    local core = (s or ""):gsub("[-+].*$", "")
    local out = {}
    for n in core:gmatch("[0-9]+") do out[#out+1] = tonumber(n) or 0 end
    return out
  end
  local pa, pb = parts(a), parts(b)
  local n = math.max(#pa, #pb)
  for i = 1, n do
    local x = pa[i] or 0
    local y = pb[i] or 0
    if x ~= y then return x - y end
  end
  return 0
end

local function extract_current_version(path, content)
  if path:match("pom%.xml$") then
    return content:match("<version>([^<]+)</version>")
  elseif path:match("package%.json$") then
    local ok, obj = pcall(arbor.json.decode, content)
    if ok and type(obj) == "table" then return tostring(obj.version or "") end
  elseif path:match("Cargo%.toml$") then
    return content:match('version%s*=%s*"([^"]+)"')
  end
  return nil
end

function M.assert_version_bump(params, ctx)
  local path        = U.abs_path(params.file or "", ctx)
  local new_version = params.new_version or ""
  local lines, add  = U.new_log("assert_version_bump")
  U.log_entry(add, "file", path)
  U.log_entry(add, "new ", new_version)
  if not arbor.fs.exists(path) then
    return U.fail(lines, "file not found: " .. path)
  end
  if new_version == "" then
    return U.fail(lines, "new_version is required")
  end
  local content = arbor.fs.read(path) or ""
  local curr = extract_current_version(path, content)
  if not curr or curr == "" then
    return U.fail(lines, "could not extract current version from " .. path)
  end
  U.log_entry(add, "curr", curr)
  local d = semver_cmp(curr, new_version)
  if d < 0 then
    add("PASS (" .. curr .. " → " .. new_version .. ")")
    return U.finish(lines)
  elseif d == 0 then
    return U.fail(lines, "same version (" .. curr .. "), not a bump")
  else
    return U.fail(lines, "new < current (" .. new_version .. " < " .. curr .. ")")
  end
end

function M.register()
  arbor.pipeline.register_op("assert_file_exists",       M.assert_file_exists)
  arbor.pipeline.register_op("assert_file_not_contains", M.assert_file_not_contains)
  arbor.pipeline.register_op("assert_glob_matches",      M.assert_glob_matches)
  arbor.pipeline.register_op("assert_version_bump",      M.assert_version_bump)
end

return M
