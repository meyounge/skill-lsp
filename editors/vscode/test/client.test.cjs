'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { serverOptions } = require('../server-options.cjs');
const root = path.resolve(__dirname, '..');

test('server launch keeps executable paths and arguments separate, without a shell', () => {
  const value = serverOptions({ serverPath: '${workspaceFolder}/bin/skill-lsp',
    fndDir: '${workspaceFolder}/private docs', database: '/local/api.json', maxCompletions: 500 },
  '/workspace', root, 'linux', 'x64', () => false);
  assert.equal(value.command, path.resolve('/workspace/bin/skill-lsp'));
  assert.deepEqual(value.args, ['--database', '/local/api.json', '--max-completions', '500']);
  assert.equal(value.options.shell, false);
  assert.equal(value.options.cwd, '/workspace');
});

test('default launch needs no document path and optional bundling respects platform', () => {
  assert.equal(serverOptions({}, undefined, root, 'linux', 'x64', () => false).command, 'skill-lsp');
  const bundled = serverOptions({}, undefined, root, 'win32', 'arm64', () => true);
  assert.equal(bundled.command, path.join(root, 'bin/win32-arm64/skill-lsp.exe'));
  assert.deepEqual(bundled.args, []);
  assert.deepEqual(serverOptions({ fndDir: 'local docs' }, undefined, root, 'linux', 'x64', () => false).args,
    ['--fnd-dir', 'local docs']);
  assert.throws(() => serverOptions({ serverPath: '${workspaceFolder}/server' }, undefined, root), /open workspace/);
  assert.throws(() => serverOptions({ maxCompletions: -1 }, undefined, root, '', '', () => false), /nonnegative/);
});

function host(trusted = true) {
  const instances = [], commands = new Map(), errors = [], lines = [];
  let onConfiguration;
  const settings = { serverPath: 'skill-lsp', fndDir: '', database: '', maxCompletions: 0 };
  class Client {
    constructor(id, name, options, clientOptions) { Object.assign(this, { id, name, options, clientOptions }); instances.push(this); }
    async start() { this.started = true; }
    async stop() { this.stopped = true; }
    async dispose() { this.disposed = true; }
  }
  const disposable = { dispose() {} };
  const vscode = {
    window: {
      createOutputChannel: () => ({ appendLine: value => lines.push(value), show() {}, dispose() {} }),
      showErrorMessage: value => errors.push(value),
    },
    workspace: {
      isTrusted: trusted, workspaceFolders: [{ uri: { fsPath: '/workspace' } }],
      getConfiguration: () => ({ get: name => settings[name] }),
      onDidChangeConfiguration: callback => { onConfiguration = callback; return disposable; },
    },
    commands: { registerCommand: (name, callback) => { commands.set(name, callback); return disposable; } },
  };
  const module = { exports: {} };
  vm.runInNewContext(fs.readFileSync(path.join(root, 'extension.cjs'), 'utf8'), {
    module, process: { platform: 'linux', arch: 'x64' },
    require(name) {
      if (name === 'node:fs') return { existsSync: () => false };
      if (name === 'vscode') return vscode;
      if (name === 'vscode-languageclient/node') return { LanguageClient: Client };
      if (name === './server-options.cjs') return { serverOptions };
      throw new Error(`Unexpected dependency ${name}`);
    },
  });
  return { ...module.exports, context: { extensionPath: root, subscriptions: [] }, instances, commands, errors, lines,
    settings, change: () => onConfiguration({ affectsConfiguration: () => true }) };
}

test('extension starts, restarts on settings changes, and stops on deactivation', async () => {
  const extension = host();
  await extension.activate(extension.context);
  assert.equal(extension.instances.length, 1);
  assert.equal(extension.instances[0].started, true);
  assert.equal(extension.instances[0].clientOptions.documentSelector[0].language, 'skill');
  extension.settings.fndDir = '/my private docs';
  extension.change();
  await new Promise(setImmediate);
  assert.equal(extension.instances[0].stopped, true);
  assert.deepEqual(extension.instances[1].options.args, ['--fnd-dir', '/my private docs']);
  await extension.commands.get('skillLsp.restart')();
  assert.equal(extension.instances.length, 3);
  await extension.deactivate();
  assert.equal(extension.instances[2].stopped, true);
  assert.deepEqual(extension.errors, []);
});

test('untrusted workspaces do not launch executables; invalid settings are reported', async () => {
  const untrusted = host(false);
  await untrusted.activate(untrusted.context);
  assert.equal(untrusted.instances.length, 0);
  const invalid = host();
  invalid.settings.maxCompletions = -1;
  await invalid.activate(invalid.context);
  assert.equal(invalid.instances.length, 0);
  assert.equal(invalid.errors.length, 1);
  assert.match(invalid.lines[0], /nonnegative/);
  await invalid.deactivate();
});

test('hover keeps parameter rows and columns, preserving prose, range, and selector diagrams', async () => {
  const extension = host();
  await extension.activate(extension.context);
  const { provideHover } = extension.instances[0].clientOptions.middleware;
  const markdown = { value: '**demoOpen**\n\n Parameters:\n   name    string\n   mode    string  [optional]\n \n Returns: object\n\n Opens an invented object.' };
  const hover = { contents: [markdown], range: { start: 1, end: 2 } };
  const document = {}, position = {}, token = {};
  const result = await provideHover(document, position, token, async (...args) => {
    assert.deepEqual(args, [document, position, token]);
    return hover;
  });
  assert.equal(result, hover);
  assert.match(markdown.value, /Parameters:\n\n```text\nname    string\nmode    string  \[optional\]\n```\n/);
  assert.ok(markdown.value.endsWith(' Returns: object\n\n Opens an invented object.'));
  assert.deepEqual(result.range, { start: 1, end: 2 });
  const formatted = markdown.value;
  await provideHover(document, position, token, async () => hover);
  assert.equal(markdown.value, formatted);
  const selector = { contents: [{ value: '**caadar**\n\n```text\n((- (x -) -) -)\n```' }] };
  const original = selector.contents[0].value;
  assert.equal(await provideHover(document, position, token, async () => selector), selector);
  assert.equal(selector.contents[0].value, original);
  assert.equal(await provideHover(document, position, token, async () => null), null);
  const plain = { contents: ['plain text', { language: 'skill', value: 'Parameters:\n  sample' }] };
  await provideHover(document, position, token, async () => plain);
  assert.equal(plain.contents[1].value, 'Parameters:\n  sample');
  await extension.deactivate();
});

test('manifest preserves the Lua palette and confines default overrides to SKILL', () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
  const colors = manifest.contributes.configurationDefaults['editor.semanticTokenColorCustomizations'].rules;
  const lua = fs.readFileSync(path.resolve(root, '../skill_colors.lua'), 'utf8');
  const palette = [...lua.matchAll(/(?:\["(function)"\]|(\w+))\s*=\s*\{ common\.color "(#[A-Fa-f0-9]{6})"/g)];
  assert.equal(palette.length, 18);
  for (const match of palette) assert.equal(colors[`${match[1] || match[2]}:skill`], match[3]);
  assert.ok(Object.keys(colors).every(key => key.endsWith(':skill')));
  const grammar = JSON.parse(fs.readFileSync(path.join(root, 'syntaxes/skill.tmLanguage.json'), 'utf8'));
  const marker = grammar.patterns.find(rule => rule.name === 'support.function.keyword7.skill');
  assert.equal(new RegExp(marker.match).exec('@optional')[0], '@optional');
  const accessor = grammar.patterns.find(rule => rule.name === 'support.function.keyword2.skill');
  assert.equal(new RegExp(accessor.match).test('caadar'), true);
  assert.equal(new RegExp(accessor.match).test('carpet'), false);
});
