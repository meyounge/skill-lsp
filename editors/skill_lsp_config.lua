-- mod-version:3

-- skill_lsp_config.lua
-- Lite-XL LSP plugin configuration for the SKILL language server.
-- Compatible with the existing style.lsp_semantic_tokens palette.
--
-- INSTALLATION:
--   1. Install lite-xl-lsp: https://github.com/lite-xl/lite-xl-lsp
--   2. Copy this file to ~/.config/lite-xl/plugins/
--   3. Copy skill_colors.lua to ~/.config/lite-xl/colors/
--   4. In your init.lua:
--        core.reload_module("colors.skill_colors")
--        require "plugins.skill_lsp_config"
--   5. Make sure skill_lsp binary is on your PATH or set the path below.

local lsp     = require "plugins.lsp"
local style   = require "core.style"
local common  = require "core.common"

-- Adjust these two paths. Leave FND_DIR nil for an embedded database.
local SKILL_LSP_BIN = USERDIR .. "/plugins/skill_lsp/" ..
    (PLATFORM == "Windows" and "skill-lsp.exe" or "skill-lsp")
local FND_DIR = nil
local command = { SKILL_LSP_BIN }
if FND_DIR then
    command[#command + 1] = "--fnd-dir"
    command[#command + 1] = FND_DIR
end

-- ---------------------------------------------------------------------------
-- SKILL syntax definition
-- Used for files before the LSP has attached, and for anything the LSP
-- doesn't emit a semantic token for (e.g. operators, bare parens).
-- ---------------------------------------------------------------------------
local syntax = require "core.syntax"

syntax.add {
    name  = "SKILL",
    files = { "%.il$", "%.ils$", "%.skill$" },
    comment = ";",
    block_comment = { "/*", "*/" },
    brackets = {
        { "(", ")", "parentheses", "balanced" },
        { "{", "}", "curly",       "balanced" },
        { "[", "]", "square",      "balanced" }
    },
    patterns = {
        -- Block comments
        { pattern = "/%*",          "*/",        type = "comment"  },
        -- Line comments
        { pattern = ";.*",                        type = "comment"  },
        -- Strings
        { pattern = '".-"',                       type = "string"   },
        -- Hex numbers
        { pattern = "-?0x[%da-fA-F]+",            type = "number"   },
        -- Float / int
        { pattern = "-?%d+%.?%d*[eE]?[+-]?%d*",  type = "number"   },
        -- Operators and parens
        { pattern = "[%(%)%[%]%{%}]",             type = "operator" },
        { pattern = "[=~%-><!&|,?\\]",            type = "operator" },
        -- Lambda-list markers use the same color before semantic tokens arrive.
        { pattern = "@[A-Za-z_][A-Za-z0-9_]*", type = "keyword7" },
        -- Identifiers (LSP semantic tokens override the color at runtime)
        { pattern = "[A-Za-z_][%w_?!*]*",         type = "symbol"   }
    },

    -- Static symbol table — covers anything not yet in the LSP database.
    -- The LSP semantic token pass will override colors for anything it knows
    -- about, so this is purely a safety net.
    symbols = {
        -- Control flow / syntax (keyword tier)
        ["if"]          = "keyword",
        ["when"]        = "keyword",
        ["unless"]      = "keyword",
        ["else"]        = "keyword",
        ["then"]        = "keyword",
        ["cond"]        = "keyword",
        ["case"]        = "keyword",
        ["caseq"]       = "keyword",
        ["foreach"]     = "keyword",
        ["for"]         = "keyword",
        ["while"]       = "keyword",
        ["loop"]        = "keyword",
        ["defun"]       = "keyword",
        ["procedure"]   = "keyword",
        ["lambda"]      = "keyword",
        ["nlambda"]     = "keyword",
        ["defmacro"]    = "keyword",
        ["defmethod"]   = "keyword",
        ["defgeneric"]  = "keyword",
        ["defclass"]    = "keyword",
        ["defstruct"]   = "keyword",
        ["defvar"]      = "keyword",
        ["let"]         = "keyword",
        ["let*"]        = "keyword",
        ["letseq"]      = "keyword",
        ["letrec"]      = "keyword",
        ["prog"]        = "keyword",
        ["progn"]       = "keyword",
        ["prog1"]       = "keyword",
        ["prog2"]       = "keyword",
        ["setq"]        = "keyword",
        ["set"]         = "keyword",
        ["return"]      = "keyword",
        ["go"]          = "keyword",
        ["break"]       = "keyword",
        ["continue"]    = "keyword",
        ["and"]         = "keyword",
        ["or"]          = "keyword",
        ["not"]         = "keyword",
        ["begin"]       = "keyword",
        ["quote"]       = "keyword",
        -- Literals
        ["t"]           = "literal",
        ["nil"]         = "literal",
    },
}

-- ---------------------------------------------------------------------------
-- Register the LSP server
-- ---------------------------------------------------------------------------
lsp.add_server {
    name          = "skill",
    language      = "skill",
    file_patterns = { "%.il$", "%.ils$", "%.skill$" },
    command       = command,

    -- Declare the semantic token types our server emits so the LSP plugin
    -- knows how to request and process them.
    semantic_tokens = true,

    -- Map each server-side token type name to a color from the theme.
    -- style.lsp_semantic_tokens is populated by skill_colors.lua.
    -- If the theme isn't loaded yet we fall back to style.syntax equivalents.
    get_color_for_token = function(token_type, _modifiers)
        if token_type == "literal" then
            return style.syntax.literal or style.syntax["normal"] or style.text
        end
        local map = style.lsp_semantic_tokens or style.syntax
        return map[token_type] or style.syntax["normal"]
    end,

    settings = {},
}

