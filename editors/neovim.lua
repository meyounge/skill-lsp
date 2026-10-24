-- Neovim 0.11+; adjust both paths. Omit --fnd-dir for an embedded database.
vim.filetype.add({ extension = { il = 'skill', ils = 'skill', skill = 'skill' } })
vim.lsp.config('skill', {
  cmd = { '/absolute/path/to/skill-lsp', '--fnd-dir', '/absolute/path/to/SKILL' },
  filetypes = { 'skill' },
  root_markers = { '.git' },
})
vim.lsp.enable('skill')
