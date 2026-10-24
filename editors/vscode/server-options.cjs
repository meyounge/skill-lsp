'use strict';

const path = require('node:path');

function expand(value, workspaceFolder) {
  if (typeof value !== 'string') throw new Error('SKILL paths must be strings.');
  if (value.includes('${workspaceFolder}')) {
    if (!workspaceFolder) throw new Error('${workspaceFolder} requires an open workspace folder.');
    return value.replaceAll('${workspaceFolder}', workspaceFolder);
  }
  return value;
}

function serverOptions(settings, workspaceFolder, extensionPath, platform, arch, exists) {
  let command = expand(settings.serverPath || 'skill-lsp', workspaceFolder);
  if (command === 'skill-lsp') {
    const bundled = path.join(extensionPath, 'bin', `${platform}-${arch}`, platform === 'win32' ? 'skill-lsp.exe' : 'skill-lsp');
    if (exists(bundled)) command = bundled;
  }
  if (!path.isAbsolute(command) && /[/\\]/.test(command)) {
    command = path.resolve(workspaceFolder || extensionPath, command);
  }
  const args = [];
  const database = expand(settings.database || '', workspaceFolder);
  const fndDir = expand(settings.fndDir || '', workspaceFolder);
  if (database) args.push('--database', database);
  else if (fndDir) args.push('--fnd-dir', fndDir);
  const limit = settings.maxCompletions ?? 0;
  if (!Number.isInteger(limit) || limit < 0) throw new Error('maxCompletions must be a nonnegative integer.');
  if (limit) args.push('--max-completions', String(limit));
  return { command, args, options: { cwd: workspaceFolder || extensionPath, shell: false } };
}

module.exports = { serverOptions };
