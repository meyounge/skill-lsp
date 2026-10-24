'use strict';

const fs = require('node:fs');
const vscode = require('vscode');
const { LanguageClient } = require('vscode-languageclient/node');
const { serverOptions } = require('./server-options.cjs');

let client;
let transition = Promise.resolve();
let disposed = false;

// The compatibility server pads parameter columns with spaces. Markdown prose
// collapses those spaces and newlines, so render just that section as code.
function formatHover(hover) {
  for (const content of hover?.contents || []) {
    if (!content || typeof content.value !== 'string' || 'language' in content) continue;
    content.value = content.value.replace(
      /^[ \t]*Parameters:\r?\n((?:[ \t]+\S[^\r\n]*(?:\r?\n|$))+)/gm,
      (_match, rows) => {
        const lines = rows.trimEnd().split(/\r?\n/);
        const indent = Math.min(...lines.map(line => line.match(/^[ \t]*/)[0].length));
        const body = lines.map(line => line.slice(indent)).join('\n');
        const ticks = body.match(/`+/g) || [];
        const fence = '`'.repeat(Math.max(3, ...ticks.map(run => run.length + 1)));
        return `Parameters:\n\n${fence}text\n${body}\n${fence}\n`;
      },
    );
  }
  return hover;
}

async function activate(context) {
  if (!vscode.workspace.isTrusted) return;
  disposed = false;
  const output = vscode.window.createOutputChannel('SKILL Language Server');
  context.subscriptions.push(output);
  async function start() {
    if (client) {
      const old = client;
      client = undefined;
      await old.stop();
    }
    if (disposed) return;
    const config = vscode.workspace.getConfiguration('skillLsp');
    const folder = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    const options = serverOptions({
      serverPath: config.get('serverPath'), fndDir: config.get('fndDir'),
      database: config.get('database'), maxCompletions: config.get('maxCompletions'),
    }, folder, context.extensionPath, process.platform, process.arch, fs.existsSync);
    const next = new LanguageClient('skillLsp', 'SKILL Language Server', options, {
      documentSelector: [{ language: 'skill', scheme: 'file' }, { language: 'skill', scheme: 'untitled' }],
      outputChannel: output,
      middleware: {
        async provideHover(document, position, token, next) {
          return formatHover(await next(document, position, token));
        },
      },
    });
    client = next;
    try { await next.start(); }
    catch (error) {
      if (client === next) client = undefined;
      await next.dispose().catch(() => {});
      throw error;
    }
  }
  function restart() {
    transition = transition.then(start).catch(error => {
      output.appendLine(String(error));
      void vscode.window.showErrorMessage('SKILL server could not start. Build/install skill-lsp and set skillLsp.serverPath. See the SKILL Language Server output for details.');
    });
    return transition;
  }
  context.subscriptions.push(
    vscode.commands.registerCommand('skillLsp.restart', restart),
    vscode.commands.registerCommand('skillLsp.showOutput', () => output.show()),
    vscode.workspace.onDidChangeConfiguration(event => {
      if (event.affectsConfiguration('skillLsp')) void restart();
    }),
    { dispose() { disposed = true; } },
  );
  await restart();
}

async function deactivate() {
  disposed = true;
  await transition;
  if (client) {
    const old = client;
    client = undefined;
    await old.stop();
  }
}

module.exports = { activate, deactivate };
