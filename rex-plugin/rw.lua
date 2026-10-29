-- rw: palette actions and keys that run the rw script in a floating terminal.
--
--   require("rw").setup{ bin = "/path/to/rw" }
--
-- Options:
--   bin      the rw executable. Rex runs actions without your interactive
--            shell's PATH, so an absolute path is safest.
--   launch   the rw-launch executable that composes a new session
--            (default: ~/.cargo/bin/rw-launch).
--   keys     { new = ..., add = ..., edit = ..., close = ... }; false for none.
--   columns  the popup's size in cells, title bar included (108 by 24).
--   rows

local M = {}

local TERMINAL = "com.superlogical.terminal"

local defaults = {
  bin = "rw",
  launch = (os.getenv("HOME") or "") .. "/.cargo/bin/rw-launch",
  keys = {
    new = "alt+space>n",
    add = "alt+space>a",
    edit = "alt+space>e",
    close = "alt+space>w",
  },
  columns = 108,
  rows = 24,
}

local function find_rect(node, block_id)
  if type(node) ~= "table" then return nil end
  if node.block_id == block_id and node.rect then return node.rect end
  for _, child in pairs(node) do
    local rect = find_rect(child, block_id)
    if rect then return rect end
  end
end

-- Layer bounds are fractions of the window, so the window's size in cells is
-- worked out from the focused pane's size and its share of the window.
local function popup_bounds(ctx, opts)
  local fallback = { 0.15, 0.2, 0.7, 0.6 }
  if ctx.block_id == nil then return fallback end
  local view = rex.session.view{}
  local rect = view and find_rect(view.windows, ctx.block_id)
  local size = rect and rex.block.call(TERMINAL, "size", { block_id = ctx.block_id })
  if size == nil then return fallback end
  local window_columns = size.columns / rect[3]
  local window_rows = size.rows / rect[4]
  local w = math.min(opts.columns, window_columns - 8) / window_columns
  local h = math.min(opts.rows, window_rows - 4) / window_rows
  return { (1 - w) / 2, (1 - h) * 0.4, w, h }
end

-- The login shell supplies PATH. A failure waits for Enter so the message
-- stays readable.
local SCRIPT = [[
"$@" || { echo; printf 'rw failed. Press Enter to close.'; read -r _; }
]]

local function popup(ctx, opts, title, bin, subcommand)
  local command = { "/bin/zsh", "-lic", SCRIPT, "rw", bin }
  for _, word in ipairs(subcommand) do table.insert(command, word) end
  for _, word in ipairs{
    "--session", ctx.session_id or "",
    "--client", ctx.client_id or "",
    "--block", ctx.block_id or "",
  } do table.insert(command, word) end
  -- No window_id: a layer created with one was placed but not drawn by the
  -- Rex app, while one in the active window was.
  local _, err = rex.session.new_layer{
    bounds = popup_bounds(ctx, opts),
    focus  = true,
    layout = {
      block = {
        flavor  = TERMINAL .. ".shell",
        label   = title,
        options = { command = command, cwd = os.getenv("HOME") },
      },
    },
  }
  if err ~= nil then error(err, 0) end
end

function M.setup(options)
  local opts = {}
  for name, value in pairs(defaults) do opts[name] = value end
  for name, value in pairs(options or {}) do opts[name] = value end
  local keys = opts.keys or {}

  local function define(name, title, subcommand, description, bin)
    rex.action{
      name        = "rw_" .. name,
      title       = "rw: " .. title,
      description = description,
      category    = "rw",
      keywords    = "rw worktree wrk " .. name,
      run         = function(ctx, args)
        popup(ctx, opts, "rw " .. name, bin or opts.bin, subcommand)
      end,
    }
    if keys[name] then rex.bind(keys[name], "rw_" .. name) end
  end

  define("new", "new session", {}, "Compose a ticket or name, a repo and an agent, then open a session in a new worktree", opts.launch)
  define("add", "add block", { "add" }, "Add an agent block to the current session's worktree")
  define("edit", "edit file", { "edit" }, "Open helix in the current session's worktree")
  define("children", "open sub-ticket sessions", { "children" }, "Open one session per sub-ticket of this session's ticket")
  define("close", "close session", { "close", "--confirm" }, "Remove the current session's worktree and close the session")
end

return M
