#!/usr/bin/env python3
"""Check native UTF-16 clients and Lite-XL UTF-32 clients with synthetic data."""
import argparse
import os
from pathlib import Path
from smoke import Client, ROOT


def run(binary, encoding):
    client = Client([str(binary), '--fnd-dir', str(ROOT/'examples/SKILL')])
    capabilities = {'general': {'positionEncodings': [encoding]}} if encoding else {}
    utf16 = encoding != 'utf-32'
    try:
        result = client.send('initialize', {'processId': None, 'capabilities': capabilities}, request=True)
        assert result['capabilities']['positionEncoding'] == ('utf-16' if utf16 else 'utf-32')
        client.send('initialized', {})
        uri = (ROOT/'examples/encoding-smoke.il').as_uri()
        text = '"😀" procedure(myFn() t)\n"😀" myFn()\n"😀" )'
        client.send('textDocument/didOpen', {'textDocument': {'uri': uri, 'languageId': 'skill', 'version': 1, 'text': text}})
        start = 5 if utf16 else 4
        position = {'textDocument': {'uri': uri}, 'position': {'line': 1, 'character': start+1}}
        hover = client.send('textDocument/hover', position, request=True)
        assert 'myFn' in hover['contents']['value']
        definition = client.send('textDocument/definition', position, request=True)
        assert definition['range']['start'] == {'line': 0, 'character': start}, definition
        references = client.send('textDocument/references', {**position, 'context': {'includeDeclaration': True}}, request=True)
        assert references[1]['range']['start'] == {'line': 1, 'character': start}
        data = client.send('textDocument/semanticTokens/full', {'textDocument': {'uri': uri}}, request=True)['data']
        assert data[2] == (4 if utf16 else 3), data
        assert data[6] == start, data
        notifications = [n['params']['diagnostics'] for n in client.notifications if n.get('method') == 'textDocument/publishDiagnostics']
        assert notifications[-1][-1]['range']['start'] == {'line': 2, 'character': start}
        client.close()
    finally:
        if client.process.poll() is None:
            client.process.kill()
            client.process.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT/'target/release'/('skill-lsp.exe' if os.name == 'nt' else 'skill-lsp'))
    binary = parser.parse_args().binary.resolve()
    for encoding in (None, 'utf-16', 'utf-32'):
        run(binary, encoding)
    print('PASS: UTF-16 default/negotiation and UTF-32 Lite-XL hover, ranges, diagnostics, and tokens')


if __name__ == '__main__':
    main()
