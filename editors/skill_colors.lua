-- skill_colors.lua
-- Standalone semantic token highlighting plugin for the SKILL language server.
-- Operates independently of lite-xl-lsp — connects to skill_lsp on its own,
-- requests semantic tokens, and overrides DocView:draw_line_text to paint
-- the keyword palette and literal colors.
--
-- This plugin handles ONLY coloring. Everything else (completion, hover,
-- diagnostics, signature help) is still handled by the lsp plugin as normal.
--
-- See README_skill_colors.md for installation and configuration.

local core      = require "core"
local style     = require "core.style"
local common    = require "core.common"
local config    = require "core.config"
local DocView   = require "core.docview"

-- ---------------------------------------------------------------------------
-- Configuration (override any of these in your init.lua before requiring
-- this plugin, or after via config.plugins.skill_colors.*)
-- ---------------------------------------------------------------------------
config.plugins.skill_colors = common.merge({
  -- Command to launch the LSP server. Must match what you have in your
  -- lsp config. Can be a full path if skill_lsp is not on $PATH.
  server_command = USERDIR .. "/plugins/skill-lsp/" ..
    (PLATFORM == "Windows" and "skill-lsp.exe" or "skill-lsp"),

  -- Directory of .fnd files, passed to the server as --fnd-dir.
  -- Set to "" to rely on the server's compiled-in default.
  fnd_dir = "",

  -- File patterns this plugin activates on.
  file_patterns = { "%.il$", "%.ils$", "%.skill$" },

  -- How many seconds to wait between re-requesting tokens after a change.
  -- Batch nearby edits for 50 ms; continuous typing cannot postpone forever.
  refresh_delay = 0.05,

  -- Print debug messages to the Lite-XL log (core.log).
  debug = false,
  -- Color used to underline matching brackets.
  bracket_color = "#39FF14",
  -- Size in pixels of the bracket underline.
  bracket_line_size = 1,

}, config.plugins.skill_colors or {})

-- ---------------------------------------------------------------------------
-- Authoritative SKILL palette — used for semantic and syntax fallback colors.
-- Edit these to taste; they must be {r, g, b[, a]} tables.
-- ---------------------------------------------------------------------------
local COLORS = {
  keyword1  = { common.color "#4FC8C0" },  -- Core SKILL builtins (no prefix)
  keyword2  = { common.color "#7AB8F0" },  -- Core SKILL prefixed
  keyword3  = { common.color "#D4A060" },  -- Database: db, dd, dr, get, hnl
  keyword4  = { common.color "#9B7FE8" },  -- Layout/CADENCE: le, hi, ge, tech
  keyword5  = { common.color "#C87FD0" },  -- PCells/Schematics: pc, sch
  keyword6  = { common.color "#E26F8F" },  -- MAE/AXL: mae, axl, asi
  keyword7  = { common.color "#E06060" },  -- ROD: rod
  keyword8  = { common.color "#60B8D0" },  -- OCEAN
  keyword9  = { common.color "#A0C040" },  -- SIM/Netlisting
  keyword10 = { common.color "#6080A8" },  -- everything else
  keyword   = { common.color "#4FC8C0" },  -- syntax / control flow (fallback)
  kwarg     = { common.color "#A8D8FF" },  -- ?keyword arguments
  ["function"] = { common.color "#60D0E8" },  -- user-defined procedures
  variable  = { common.color "#A0C8A0" },  -- user-defined variables
  string    = { common.color "#78C878" },
  number    = { common.color "#F1EE8E" },
  comment   = { common.color "#4A5568" },
  operator  = { common.color "#C8E050" },
}

-- Share the exact same colors with the SKILL LSP configuration callbacks.
-- Do not change style.syntax globally: other languages keep their own theme.
style.lsp_semantic_tokens = style.lsp_semantic_tokens or {}
for name, color in pairs(COLORS) do
  style.lsp_semantic_tokens[name] = color
end

-- ---------------------------------------------------------------------------
-- JSON encoder / decoder (minimal — only what we need)
-- Reuse the one bundled with the lsp plugin if available.
-- ---------------------------------------------------------------------------
local ok, lsp_json = pcall(require, "plugins.lsp.json")
if not ok then
  core.log("[skill_colors] Cannot load plugins.lsp.json: " .. tostring(lsp_json))
  return
end
local json = lsp_json

-- Lite-XL's bundled encoder serializes an empty Lua table as {}, not [].
-- Advertise nonempty arrays so tower-lsp receives the required JSON types.
local SUPPORTED_TOKEN_TYPES = {
  "keyword", "keyword1", "keyword2", "keyword3", "keyword4", "keyword5",
  "keyword6", "keyword7", "keyword8", "keyword9", "keyword10", "function",
  "variable", "comment", "string", "number", "operator", "kwarg",
  "literal",
}

-- ---------------------------------------------------------------------------
-- State
-- ---------------------------------------------------------------------------

-- Per-document semantic token maps:
--   doc_tokens[doc] = {
--     [line_number] = { {col, len, color_table}, ... }
--   }
local doc_tokens   = setmetatable({}, { __mode = "k" })  -- weak keys

-- Per-document dirty flags and pending refresh timers
local doc_dirty    = setmetatable({}, { __mode = "k" })
local doc_timer    = setmetatable({}, { __mode = "k" })
local doc_revision = setmetatable({}, { __mode = "k" })
local doc_pending  = setmetatable({}, { __mode = "k" })

-- The single long-lived server connection
local server_proc  = nil
local server_ready = false
local req_id       = 0
local pending_reqs = {}  -- id -> {uri, doc}

-- Token type list received from the server during initialization
local token_types  = {}

-- Open documents we have told the server about: uri -> version
local open_docs    = {}

-- ---------------------------------------------------------------------------
-- Helpers
-- ---------------------------------------------------------------------------

local cfg = config.plugins.skill_colors

local function dbg(fmt, ...)
  if cfg.debug then
    core.log("[skill_colors] " .. string.format(fmt, ...))
  end
end

local function matches_skill(filename)
  if not filename then return false end
  for _, pat in ipairs(cfg.file_patterns) do
    if filename:match(pat) then return true end
  end
  return false
end

local function doc_uri(doc)
  local path = doc.abs_filename or doc.filename or ""
  -- Simple URI encode: spaces and special chars
  path = path:gsub("([^%w%.%-%_~/])", function(c)
    return string.format("%%%02X", string.byte(c))
  end)
  return "file://" .. path
end

-- ---------------------------------------------------------------------------
-- JSON-RPC framing
-- ---------------------------------------------------------------------------

local function rpc_encode(payload)
  local body = json.encode(payload)
  return string.format("Content-Length: %d\r\n\r\n%s", #body, body)
end

local next_id = 0
local function new_id()
  next_id = next_id + 1
  return next_id
end

-- ---------------------------------------------------------------------------
-- Server process management
-- ---------------------------------------------------------------------------

local read_buf = ""

local function proc_write(msg)
  if not server_proc or not server_proc:running() then return false end
  local ok, err = server_proc:write(msg)
  if not ok then
    dbg("write error: %s", tostring(err))
    return false
  end
  return true
end

-- Read all available stdout into read_buf, then extract complete messages.
-- Returns a list of decoded JSON objects.
local function proc_read_messages()
  if not server_proc or not server_proc:running() then return {} end

  while true do
    local chunk = server_proc:read_stdout(8192)
    if not chunk or #chunk == 0 then break end
    dbg("Read %d bytes from server", #chunk)
    read_buf = read_buf .. chunk
  end

  local messages = {}
  while true do
    local header_end = read_buf:match("\r\n\r\n()")
    if not header_end then break end
    local header_block = read_buf:sub(1, header_end - 1)
    local length = tonumber(header_block:match("Content%-Length: (%d+)"))
    if not length then read_buf = read_buf:sub(header_end); break end
    local body_end = header_end + length - 1
    if #read_buf < body_end then break end  -- incomplete body yet

    local body = read_buf:sub(header_end, body_end)
    read_buf = read_buf:sub(body_end + 1)

    -- Decode JSON — use pcall so a bad message doesn't crash the plugin
    local ok_d, decoded = pcall(json.decode, body)
    if ok_d and decoded then
      table.insert(messages, decoded)
    else
      dbg("JSON decode error on: %s", body:sub(1, 200))
    end
  end
  return messages
end

-- ---------------------------------------------------------------------------
-- LSP handshake
-- ---------------------------------------------------------------------------

local function send_initialize(workspace)
  local id = new_id()
  local cmd = {
    jsonrpc = "2.0",
    id      = id,
    method  = "initialize",
    params  = {
      processId  = nil,
      rootUri    = "file://" .. workspace,
      capabilities = {
        general = { positionEncodings = { "utf-32" } },
        textDocument = {
          semanticTokens = {
            requests     = { full = true },
            tokenTypes   = SUPPORTED_TOKEN_TYPES,
            tokenModifiers = { "declaration" },
            formats      = { "relative" },
            multilineTokenSupport = true,
          }
        }
      }
    }
  }
  dbg("Sending initialize (id=%d)", id)
  proc_write(rpc_encode(cmd))
  return id
end

local function send_initialized()
  proc_write(rpc_encode({
    jsonrpc = "2.0",
    method  = "initialized",
    params  = {}
  }))
end

local function send_did_open(uri, text, version)
  proc_write(rpc_encode({
    jsonrpc = "2.0",
    method  = "textDocument/didOpen",
    params  = {
      textDocument = {
        uri        = uri,
        languageId = "skill",
        version    = version or 1,
        text       = text,
      }
    }
  }))
end

local function send_did_change(uri, text, version)
  proc_write(rpc_encode({
    jsonrpc = "2.0",
    method  = "textDocument/didChange",
    params  = {
      textDocument   = { uri = uri, version = version },
      contentChanges = { { text = text } },
    }
  }))
end

local function send_did_close(uri)
  proc_write(rpc_encode({
    jsonrpc = "2.0",
    method  = "textDocument/didClose",
    params  = { textDocument = { uri = uri } }
  }))
end

local function request_semantic_tokens(uri, doc)
  if doc_pending[doc] then return end
  local id = new_id()
  doc_pending[doc] = id
  local lines = {}
  for i, text in ipairs(doc.lines) do lines[i] = text end
  pending_reqs[id] = { uri = uri, doc = doc, lines = lines,
    revision = doc_revision[doc] or 0 }
  local sent = proc_write(rpc_encode({
    jsonrpc = "2.0",
    id      = id,
    method  = "textDocument/semanticTokens/full",
    params  = { textDocument = { uri = uri } }
  }))
  if not sent then
    pending_reqs[id] = nil
    doc_pending[doc] = nil
  end
  dbg("Requested tokens for %s (req id=%d)", uri, id)
  return id
end

-- ---------------------------------------------------------------------------
-- Token decoding
-- ---------------------------------------------------------------------------

-- Decode the flat LSP delta-encoded token array into our per-line structure.
-- LSP format: groups of 5 integers
--   [deltaLine, deltaStartChar, length, tokenTypeIndex, tokenModifiers]
local function decode_tokens(data, source_lines)
  local result = {}   -- result[line_number] = { {col, len, color}, ... }
  local byte_offsets = {}

  local cur_line = 0
  local cur_col  = 0

  for i = 1, #data, 5 do
    local delta_line = data[i]
    local delta_col  = data[i + 1]
    local length     = data[i + 2]
    local type_idx   = data[i + 3]
    -- data[i+4] is modifiers — we don't use them

    cur_line = cur_line + delta_line
    if delta_line > 0 then cur_col = 0 end
    cur_col = cur_col + delta_col

    local type_name = token_types[type_idx + 1]  -- Lua is 1-indexed
    local color     = type_name == "literal" and
      (style.syntax.literal or style.syntax.normal or style.text) or
      (type_name and COLORS[type_name])
    if color then
      local ln = cur_line + 1   -- Lite-XL lines are 1-indexed
      if not result[ln] then result[ln] = {} end
      local spans = result[ln]
      local col = cur_col + 1
      -- This server preserves Python's code-point columns; Lite-XL edits and
      -- drawing use UTF-8 bytes. Convert once per non-ASCII source line.
      local source = source_lines[ln] or ""
      local offsets = byte_offsets[ln]
      if offsets == nil then
        offsets = false
        if source:find("[\128-\255]") then
          offsets = {}
          local character = 0
          for byte = 1, #source do
            local value = source:byte(byte)
            if value < 128 or value >= 192 then
              offsets[character] = byte
              character = character + 1
            end
          end
          offsets[character] = #source + 1
        end
        byte_offsets[ln] = offsets
      end
      if offsets then
        col = offsets[cur_col] or (#source + 1)
        length = (offsets[cur_col + length] or (#source + 1)) - col
      end
      local last = spans[#spans]
      if last and last.color == color and last.col + last.len == col then
        last.len = last.len + length
      else
        spans[#spans + 1] = { col = col, len = length, color = color }
      end
    end
  end
  return result
end

-- ---------------------------------------------------------------------------
-- Message dispatch
-- ---------------------------------------------------------------------------

local init_req_id = nil

local function handle_message(msg)
  -- Response to our initialize request
  if msg.id and msg.id == init_req_id then
    if msg.error then
      server_ready = false
      core.log("[skill_colors] Server initialization failed: " ..
        tostring(msg.error.message or msg.error.code or "unknown error"))
      return
    end
    if not msg.result then return end
    local caps = msg.result.capabilities or {}
    local legend = (caps.semanticTokensProvider or {}).legend or {}
    token_types = legend.tokenTypes or {}
    if #token_types == 0 then
      core.log("[skill_colors] Server did not provide a semantic token legend")
      return
    end
    dbg("Initialized. Token types: %s", table.concat(token_types, ", "))
    send_initialized()
    server_ready = true
    return
  end

  -- Response to a semantic tokens request
  if msg.id and pending_reqs[msg.id] then
    local req = pending_reqs[msg.id]
    pending_reqs[msg.id] = nil
    doc_pending[req.doc] = nil

    -- An older response must not paint positions from before an edit or close.
    if not open_docs[req.uri] or req.revision ~= (doc_revision[req.doc] or 0) then
      return
    end

    if msg.result and msg.result.data then
      local data = msg.result.data
      dbg("Got %d token entries for %s", #data // 5, req.uri)
      -- Decode and store — doc may have been GC'd, guard with pcall
      local ok_dec, decoded = pcall(decode_tokens, data, req.lines)
      if ok_dec then
        doc_tokens[req.doc] = decoded
        core.redraw = true
      else
        dbg("Decode error: %s", tostring(decoded))
      end
    elseif msg.error then
      dbg("Token error for %s: %s", req.uri, msg.error.message or "?")
    end
    return
  end

  -- Ignore everything else (diagnostics from the lsp plugin handle those)
end

-- ---------------------------------------------------------------------------
-- Server startup
-- ---------------------------------------------------------------------------

local function start_server()
  if server_proc and server_proc:running() then return end

  local cmd = { cfg.server_command }
  if cfg.fnd_dir and #cfg.fnd_dir > 0 then
    cmd[#cmd+1] = "--fnd-dir"
    cmd[#cmd+1] = cfg.fnd_dir
  end

  dbg("Starting server: %s", table.concat(cmd, " "))

  local ok_s, proc_or_err = pcall(process.start, cmd, {
    stderr = process.REDIRECT_DISCARD
  })

  if not ok_s or not proc_or_err then
    core.log("[skill_colors] Failed to start skill_lsp: " .. tostring(proc_or_err))
    return
  end

  server_proc  = proc_or_err
  server_ready = false
  read_buf     = ""
  pending_reqs = {}
  open_docs    = {}
  token_types  = {}
  doc_pending = setmetatable({}, { __mode = "k" })
  doc_tokens = setmetatable({}, { __mode = "k" })

  -- Send initialize — use the first project directory as workspace
  local workspace = core.project and core.project.path
                    or (core.root_project and core.root_project.path)
                    or os.getenv("HOME") or "/"
  init_req_id = send_initialize(workspace)
end

-- ---------------------------------------------------------------------------
-- Document lifecycle helpers
-- ---------------------------------------------------------------------------

local function doc_text(doc)
  return table.concat(doc.lines)
end

local function ensure_open(doc)
  local uri = doc_uri(doc)
  if not open_docs[uri] then
    open_docs[uri] = 1
    send_did_open(uri, doc_text(doc), 1)
    doc_dirty[doc], doc_timer[doc] = nil, nil
    request_semantic_tokens(uri, doc)
    dbg("Opened %s", uri)
  end
end

local function schedule_refresh(doc)
  -- Keep the first deadline so a burst of edits cannot starve highlighting.
  doc_revision[doc] = (doc_revision[doc] or 0) + 1
  doc_dirty[doc] = true
  doc_timer[doc] = doc_timer[doc] or (system.get_time() + cfg.refresh_delay)
end

-- ---------------------------------------------------------------------------
-- Background processing coroutine
-- (Lite-XL cooperative threading via core.add_thread)
-- ---------------------------------------------------------------------------

local function background_thread()
  while true do
    coroutine.yield(0.02)   -- poll responses without adding a 50 ms frame delay

    -- Start the server if it isn't running
    if not server_proc or not server_proc:running() then
      server_proc  = nil
      server_ready = false
      start_server()
    end

    -- Pump incoming messages
    local msgs = proc_read_messages()
    for _, msg in ipairs(msgs) do
      handle_message(msg)
    end

    if not server_ready then goto continue end

    -- Process any documents that need a token refresh
    local now = system.get_time()
    for _, doc in ipairs(core.docs) do
      if not matches_skill(doc.abs_filename or doc.filename) then goto next_doc end

      -- Make sure the document is open on the server
      ensure_open(doc)

      -- If dirty and the debounce timer has expired, send a change + request
      if doc_dirty[doc] and doc_timer[doc] and now >= doc_timer[doc]
          and not doc_pending[doc] then
        doc_dirty[doc] = false
        doc_timer[doc] = nil

        local uri     = doc_uri(doc)
        local version = open_docs[uri] + 1
        open_docs[uri] = version
        send_did_change(uri, doc_text(doc), version)
        request_semantic_tokens(uri, doc)
      elseif not doc_tokens[doc] and not doc_dirty[doc] and not doc_pending[doc] then
        request_semantic_tokens(doc_uri(doc), doc)
      end

      ::next_doc::
    end

    -- Clean up closed documents
    local alive_uris = {}
    for _, doc in ipairs(core.docs) do
      if matches_skill(doc.abs_filename or doc.filename) then
        alive_uris[doc_uri(doc)] = true
      end
    end
    for uri in pairs(open_docs) do
      if not alive_uris[uri] then
        send_did_close(uri)
        open_docs[uri] = nil
        dbg("Closed %s", uri)
      end
    end

    ::continue::
  end
end

-- ---------------------------------------------------------------------------
-- Hook into document save/change events
-- ---------------------------------------------------------------------------

-- Intercept Doc:save and Doc:on_text_change to schedule refreshes.
-- We do this by wrapping the methods on the Doc class.

local Doc = require "core.doc"

-- Lite-XL 2.1+ uses Doc:on_close, older uses finalizer patterns.
-- The safest cross-version hook is wrapping the save/change methods.

local _orig_doc_save = Doc.save
function Doc:save(...)
  local result = _orig_doc_save(self, ...)
  if matches_skill(self.abs_filename or self.filename) then
    schedule_refresh(self)
  end
  return result
end

-- Hook text changes through the highlighter invalidation path —
-- every text change calls doc:push_undo which updates self._revision.
local _orig_push_undo = Doc.push_undo
if _orig_push_undo then
  function Doc:push_undo(...)
    local result = _orig_push_undo(self, ...)
    if matches_skill(self.abs_filename or self.filename) then
      schedule_refresh(self)
    end
    return result
  end
end

-- Also hook raw_insert / raw_remove which are the lowest-level change paths
-- Keep cached colors attached to surviving text while the server recomputes.
-- These hooks are also called by undo/redo. New text falls back to syntax color
-- until a fresh semantic response arrives.
local function append_span(spans, col, len, color)
  if len <= 0 then return end
  local last = spans[#spans]
  if last and last.color == color and last.col + last.len == col then
    last.len = last.len + len
  else
    spans[#spans + 1] = { col = col, len = len, color = color }
  end
end

local function shift_insert(doc, line, col, text)
  local tokens = doc_tokens[doc]
  if not tokens or text == "" then return end
  local _, added = text:gsub("\n", "")
  local tail = text:match("([^\n]*)$")
  local first, last = {}, {}
  if added == 0 then last = first end
  for _, span in ipairs(tokens[line] or {}) do
    local finish = span.col + span.len
    local left = math.max(0, math.min(finish, col) - span.col)
    append_span(first, span.col, left, span.color)
    local right = math.max(span.col, col)
    if finish > right then
      local destination = added == 0 and (right + #text) or (#tail + 1 + right - col)
      append_span(last, destination, finish - right, span.color)
    end
  end
  if added > 0 then
    local shifted = {}
    for ln, spans in pairs(tokens) do
      if ln < line then shifted[ln] = spans
      elseif ln > line then shifted[ln + added] = spans end
    end
    shifted[line], shifted[line + added] = first, last
    doc_tokens[doc] = shifted
  else
    tokens[line] = first
  end
  core.redraw = true
end

local function shift_remove(doc, line1, col1, line2, col2)
  local tokens = doc_tokens[doc]
  if not tokens or (line1 == line2 and col1 == col2) then return end
  local joined = {}
  for _, span in ipairs(tokens[line1] or {}) do
    append_span(joined, span.col, math.min(span.col + span.len, col1) - span.col, span.color)
  end
  for _, span in ipairs(tokens[line2] or {}) do
    local right = math.max(span.col, col2)
    append_span(joined, col1 + right - col2, span.col + span.len - right, span.color)
  end
  local removed = line2 - line1
  if removed > 0 then
    local shifted = {}
    for ln, spans in pairs(tokens) do
      if ln < line1 then shifted[ln] = spans
      elseif ln > line2 then shifted[ln - removed] = spans end
    end
    shifted[line1] = joined
    doc_tokens[doc] = shifted
  else
    tokens[line1] = joined
  end
  core.redraw = true
end

local _orig_raw_insert = Doc.raw_insert
if _orig_raw_insert then
  function Doc:raw_insert(line, col, text, ...)
    local result = _orig_raw_insert(self, line, col, text, ...)
    if matches_skill(self.abs_filename or self.filename) then
      shift_insert(self, line, col, text)
      schedule_refresh(self)
    end
    return result
  end
end

local _orig_raw_remove = Doc.raw_remove
if _orig_raw_remove then
  function Doc:raw_remove(line1, col1, line2, col2, ...)
    local result = _orig_raw_remove(self, line1, col1, line2, col2, ...)
    if matches_skill(self.abs_filename or self.filename) then
      shift_remove(self, line1, col1, line2, col2)
      schedule_refresh(self)
    end
    return result
  end
end

-- ---------------------------------------------------------------------------
-- Bracket matching (algorithm lifted directly from bracketmatch.lua)
-- ---------------------------------------------------------------------------

local bracket_maps = {
  { [91]=93, [40]=41, [123]=125, direction =  1 },  -- open -> close
  { [93]=91, [41]=40, [125]=123, direction = -1 },  -- close -> open
}

local bm_state = setmetatable({}, { __mode = "k" })

local function bm_get_token_at(doc, line, col)
  local column = 0
  for _, tok_type, text in doc.highlighter:each_token(line) do
    column = column + #text
    if column >= col then return tok_type end
  end
end

local function bm_get_matching(doc, line, col, open_byte, close_byte, direction)
  local end_line = math.max(1, math.min(#doc.lines, line + 1000 * direction))
  local depth = 0
  while line ~= end_line do
    local byte = doc.lines[line]:byte(col)
    if byte == open_byte and bm_get_token_at(doc, line, col) ~= "comment" then
      depth = depth + 1
    elseif byte == close_byte and bm_get_token_at(doc, line, col) ~= "comment" then
      depth = depth - 1
      if depth == 0 then return line, col end
    end
    local pl, pc = line, col
    line, col = doc:position_offset(line, col, direction)
    if line == pl and col == pc then break end
  end
end

local function update_bracket_state(dv)
  local doc = dv.doc
  if not doc or not matches_skill(doc.abs_filename or doc.filename) then
    bm_state[dv] = nil
    return
  end

  local line, col = doc:get_selection()
  local change_id = doc:get_change_id()
  local st = bm_state[dv]
  if st and st.line == line and st.col == col and st.change_id == change_id then
    return
  end

  local br_line, br_col, m_line, m_col
  for _, map in ipairs(bracket_maps) do
    for i = 0, -1, -1 do
      local cl, cc = doc:position_offset(line, col, i)
      local open   = doc.lines[cl]:byte(cc)
      local close  = map[open]
      if close and bm_get_token_at(doc, cl, cc) ~= "comment" then
        m_line, m_col = bm_get_matching(doc, cl, cc, open, close, map.direction)
        if m_line then
          br_line, br_col = cl, cc
          goto done
        end
      end
    end
  end
  ::done::

  bm_state[dv] = {
    line = line, col = col, change_id = change_id,
    br_line = br_line, br_col = br_col,
    match_line = m_line, match_col = m_col,
  }
end

local function draw_bracket_underline(dv, x, y, line, col)
  local r, g, b, a = common.color(cfg.bracket_color or "#FFD080")
  local bcolor = {r, g, b, a}
  local h  = math.max(1, math.ceil((cfg.bracket_line_size or 1) * SCALE))
  local lh = dv:get_line_height()
  local x1 = x + dv:get_col_x_offset(line, col)
  local x2 = x + dv:get_col_x_offset(line, col + 1)
  renderer.draw_rect(x1, y + lh - h, x2 - x1, h, bcolor)
end

local orig_docview_update = DocView.update
function DocView:update(...)
  orig_docview_update(self, ...)
  update_bracket_state(self)
end

-- ---------------------------------------------------------------------------
-- Override DocView:draw_line_text
-- This is where the actual color substitution happens.
-- ---------------------------------------------------------------------------

local orig_draw_line_text = DocView.draw_line_text

function DocView:draw_line_text(line, x, y)
  local doc = self.doc
  if not matches_skill(doc.abs_filename or doc.filename) then
    return orig_draw_line_text(self, line, x, y)
  end

  local tokens_for_doc = doc_tokens[doc]
  -- Use this palette for syntax colors even before semantic tokens arrive.
  local line_tokens = tokens_for_doc and tokens_for_doc[line]

  local default_font  = self:get_font()
  local tx = x
  local ty = y + self:get_line_text_y_offset()

  -- Semantic spans are decoded once per response. Walk their boundaries rather
  -- than allocating and scanning a per-character color table on every redraw.
  local span_index = 1

  -- Walk the highlighter's token stream.
  -- For each token we either use our semantic color or the default syntax color.
  local lh = self:get_line_height()
  local tokens_data  = doc.highlighter:get_line(line).tokens
  local tokens_count = #tokens_data
  local last_token   = nil
  if tokens_count > 0 and string.sub(tokens_data[tokens_count], -1) == "\n" then
    last_token = tokens_count - 1
  end

  local byte_pos = 1   -- current byte position within the line

  for tidx, tok_type, text in doc.highlighter:each_token(line) do
    local font  = style.syntax_fonts[tok_type] or default_font

    -- Strip the trailing newline from the last token
    local render_text = text
    if tidx == last_token then
      render_text = text:sub(1, -2)
    end

    if #render_text > 0 then
      local default_color = COLORS[tok_type] or style.syntax[tok_type]
                            or style.syntax["normal"]
                            or style.text
      local seg_byte = byte_pos
      local token_end = byte_pos + #render_text
      while seg_byte < token_end do
        local span = line_tokens and line_tokens[span_index]
        while span and span.col + span.len <= seg_byte do
          span_index = span_index + 1
          span = line_tokens[span_index]
        end
        local seg_color, seg_end = default_color, token_end
        if span then
          if span.col <= seg_byte then
            seg_color = span.color
            seg_end = math.min(token_end, span.col + span.len)
          else
            seg_end = math.min(token_end, span.col)
          end
        end
        local piece = render_text:sub(seg_byte - byte_pos + 1, seg_end - byte_pos)
        if #piece > 0 then
          tx = renderer.draw_text(font, piece, tx, ty, seg_color)
        end
        seg_byte = seg_end
      end
    end

    byte_pos = byte_pos + #text

    if tx > self.position.x + self.size.x then break end
  end

  -- Draw bracket match underlines if either bracket is on this line
  local st = bm_state[self]
  if st and st.match_line then
    if line == st.br_line then
      draw_bracket_underline(self, x, y, line, st.br_col)
    end
    if line == st.match_line then
      draw_bracket_underline(self, x, y, line, st.match_col)
    end
  end

  return lh
end

-- ---------------------------------------------------------------------------
-- Expose a manual refresh command
-- ---------------------------------------------------------------------------

local command = require "core.command"

command.add("core.docview", {
  ["skill-colors:refresh"] = function()
    local doc = core.active_view.doc
    if matches_skill(doc.abs_filename or doc.filename) then
      schedule_refresh(doc)
      core.log("[skill_colors] Token refresh scheduled")
    end
  end
})

-- Bind to F5 by default — change in your keymap if you prefer something else
local keymap = require "core.keymap"
keymap.add { ["f5"] = "skill-colors:refresh" }

-- ---------------------------------------------------------------------------
-- Start
-- ---------------------------------------------------------------------------

core.add_thread(background_thread)
start_server()

dbg("Plugin loaded. Server command: %s", cfg.server_command)
