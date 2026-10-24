#!/usr/bin/env python3
"""End-to-end stdio test using only Python's standard library.
Run after cargo build --release. Also exercises the compiled JSON database.
"""
import argparse
import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time

ROOT = Path(__file__).resolve().parents[1]

class Client:
    def __init__(self, command):
        self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, cwd=ROOT)
        self.incoming = queue.Queue()
        self.errors = []
        self.sequence = 0
        self.notifications = []
        threading.Thread(target=self._read, daemon=True).start()
        threading.Thread(target=self._errors, daemon=True).start()

    def _errors(self):
        for line in self.process.stderr:
            self.errors.append(line.decode(errors='replace').rstrip())

    def _read(self):
        try:
            while True:
                headers = {}
                while True:
                    line = self.process.stdout.readline()
                    if not line:
                        raise EOFError('server stdout closed')
                    if line == b'\r\n':
                        break
                    key, value = line.decode('ascii').split(':', 1)
                    headers[key.lower()] = value.strip()
                remaining = int(headers['content-length'])
                chunks = []
                while remaining:
                    part = self.process.stdout.read(remaining)
                    if not part:
                        raise EOFError('incomplete response')
                    chunks.append(part)
                    remaining -= len(part)
                self.incoming.put(json.loads(b''.join(chunks)))
        except Exception as exc:
            self.incoming.put(exc)

    def send(self, method, params=None, request=False):
        data = {'jsonrpc': '2.0', 'method': method}
        if params is not None:
            data['params'] = params
        if request:
            self.sequence += 1
            data['id'] = self.sequence
        payload = json.dumps(data).encode()
        self.process.stdin.write(f'Content-Length: {len(payload)}\r\n\r\n'.encode() + payload)
        self.process.stdin.flush()
        if request:
            deadline = time.monotonic() + 15
            while True:
                message = self.incoming.get(timeout=max(0.01, deadline - time.monotonic()))
                if isinstance(message, Exception):
                    raise RuntimeError(f'{message}: {self.errors}')
                if message.get('id') == self.sequence:
                    assert 'error' not in message, message
                    return message.get('result')
                self.notifications.append(message)

    def close(self):
        self.send('shutdown', request=True)
        self.send('exit')
        assert self.process.wait(timeout=10) == 0, self.errors


def run(binary, source_args):
    client = Client([str(binary), *source_args])
    try:
        init = client.send('initialize', {'processId': None, 'rootUri': ROOT.as_uri(),
                                        'capabilities': {'general': {'positionEncodings': ['utf-32']}}}, request=True)
        legend = init['capabilities']['semanticTokensProvider']['legend']['tokenTypes']
        assert legend[1:11] == [f'keyword{i}' for i in range(1, 11)]
        client.send('initialized', {})
        uri = (ROOT / 'examples' / 'smoke.il').as_uri()
        text = ('procedure(myFn(cell)\n dbSynthetic(cell "name")\n)\n'
                '; myFn in a comment\nmyFn(nil)\n')
        client.send('textDocument/didOpen', {'textDocument': {
            'uri': uri, 'languageId': 'skill', 'version': 1, 'text': text}})
        target = {'textDocument': {'uri': uri}, 'position': {'line': 1, 'character': 5}}
        hover = client.send('textDocument/hover', target, request=True)
        assert 'dbSynthetic' in hover['contents']['value'], hover
        complete = client.send('textDocument/completion', target, request=True)
        item = next(x for x in complete['items'] if x['label'] == 'dbSynthetic')
        assert 'Invented database helper' in item['documentation']['value']
        position = {'textDocument': {'uri': uri}, 'position': {'line': 4, 'character': 2}}
        definition = client.send('textDocument/definition', position, request=True)
        assert definition['range']['start'] == {'line': 0, 'character': 0}, definition
        refs = client.send('textDocument/references', {**position, 'context': {'includeDeclaration': False}}, request=True)
        assert len(refs) == 3, refs
        symbols = client.send('textDocument/documentSymbol', {'textDocument': {'uri': uri}}, request=True)
        assert any(s['name'] == 'myFn' for s in symbols)
        semantic = client.send('textDocument/semanticTokens/full', {'textDocument': {'uri': uri}}, request=True)
        assert semantic['data'] and len(semantic['data']) % 5 == 0
        edits = client.send('textDocument/formatting', {'textDocument': {'uri': uri},
            'options': {'tabSize': 2, 'insertSpaces': True}}, request=True)
        assert edits and '\n  dbSynthetic' in edits[0]['newText'], edits
        # Preserve Python's same-line Lisp-style signature help.
        changed = '(dbSynthetic nil, "name"'
        client.send('textDocument/didChange', {'textDocument': {'uri': uri, 'version': 2},
            'contentChanges': [{'text': changed}]})
        sig = client.send('textDocument/signatureHelp', {'textDocument': {'uri': uri},
            'position': {'line': 0, 'character': len(changed)}}, request=True)
        assert sig['activeParameter'] == 1, sig
        # Python accepts decreasing versions and uses the last full-text update.
        client.send('textDocument/didChange', {'textDocument': {'uri': uri, 'version': 1},
            'contentChanges': [{'text': 'stale'}]})
        sig2 = client.send('textDocument/signatureHelp', {'textDocument': {'uri': uri},
            'position': {'line': 0, 'character': 5}}, request=True)
        assert sig2 is None
        assert any(n.get('method') == 'textDocument/publishDiagnostics' and
                   n['params']['diagnostics']
                   for n in client.notifications), client.notifications
        # Python's handler columns count Unicode code points.
        changed = 'procedure(myFn() t)\n"😀" myFn()'
        client.send('textDocument/didChange', {'textDocument': {'uri': uri, 'version': 3},
            'contentChanges': [{'text': changed}]})
        unicode_hover = client.send('textDocument/hover', {'textDocument': {'uri': uri},
            'position': {'line': 1, 'character': 6}}, request=True)
        assert 'myFn' in unicode_hover['contents']['value'] and 'range' not in unicode_hover, unicode_hover
        client.send('textDocument/didClose', {'textDocument': {'uri': uri}})
        assert client.send('textDocument/hover', position, request=True) is None
        client.close()
    finally:
        if client.process.poll() is None:
            client.process.kill()
            client.process.wait()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, default=ROOT / 'target' / 'release' /
                        ('skill-lsp.exe' if os.name == 'nt' else 'skill-lsp'))
    args = parser.parse_args()
    binary = args.binary.resolve()
    assert binary.is_file(), f'Build first: cargo build --release ({binary})'
    run(binary, ['--fnd-dir', str(ROOT / 'examples' / 'SKILL')])
    import tempfile
    with tempfile.TemporaryDirectory(dir=ROOT / 'target') as tmp:
        database = Path(tmp) / 'api.json'
        subprocess.run([str(binary), 'index', str(ROOT / 'examples' / 'SKILL'), str(database)], check=True)
        actual = json.loads(database.read_text())
        expected = json.loads((ROOT / 'tests' / 'expected_api.json').read_text())
        normalize = lambda entries: [{k: v for k, v in e.items() if k != 'source_file'}
                                     for e in sorted(entries, key=lambda e: e['name'])]
        assert normalize(actual) == normalize(expected), 'Rust output differs from original Python on fixtures'
        run(binary, ['--database', str(database)])
        # The Python server returns all matches; the old Rust default capped at 300.
        expanded=actual+[dict(actual[0],name=f'inventedCompletion{i}') for i in range(350)]
        database.write_text(json.dumps(expanded))
        client=Client([str(binary),'--database',str(database)])
        try:
            client.send('initialize',{'processId':None,'capabilities':{}},request=True)
            client.send('initialized',{})
            complete=client.send('textDocument/completion',{'textDocument':{'uri':(ROOT/'examples/empty.il').as_uri()},
                'position':{'line':0,'character':0}},request=True)
            assert len(complete['items'])>350 and not complete['isIncomplete'],len(complete['items'])
            client.close()
        finally:
            if client.process.poll() is None:
                client.process.kill()
                client.process.wait()
    print('PASS: stdio features, Python-compatible updates/columns, runtime/index parity, Python parser fixture parity')

if __name__ == '__main__':
    main()
