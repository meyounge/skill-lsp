#!/usr/bin/env python3
"""Compare the supplied Python binary and Rust server over actual LSP messages.

Only synthetic Finder records are used. Completion ordering is normalized because
Python builds keywords from a randomly ordered set. Nothing else is normalized.
"""
import argparse
import json
import random
from pathlib import Path
import statistics
import sys
import time
from smoke import Client, ROOT

def canonical(value, method):
    if method == 'textDocument/completion' and value:
        value = dict(value)
        value['items'] = sorted(value['items'], key=lambda x: json.dumps(x, sort_keys=True))
    return value

def compare(results, context):
    if results[0] == results[1]:
        return
    if isinstance(results[0],dict) and 'items' in results[0]:
        expected={x['label']:x for x in results[0]['items']}
        actual={x['label']:x for x in results[1]['items']}
        differences=[(k,expected.get(k),actual.get(k)) for k in sorted(expected.keys()|actual.keys())
                     if expected.get(k)!=actual.get(k)]
        raise AssertionError((context,differences[:2]))
    raise AssertionError((context,results))

def connect(command):
    client = Client(command)
    init = client.send('initialize', {'processId': None, 'rootUri': ROOT.as_uri(),
                                    'capabilities': {'general': {'positionEncodings': ['utf-32']}}}, request=True)
    client.send('initialized', {})
    return client, init

def query(client, method, uri, line=0, column=0):
    params = {'textDocument': {'uri': uri}}
    if method in ('textDocument/completion', 'textDocument/hover', 'textDocument/definition',
                  'textDocument/references', 'textDocument/signatureHelp'):
        params['position'] = {'line': line, 'character': column}
    if method == 'textDocument/references':
        params['context'] = {'includeDeclaration': False}
    if method == 'textDocument/formatting':
        params['options'] = {'tabSize': 2, 'insertSpaces': False}
    return canonical(client.send(method, params, request=True), method)

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--python-binary', type=Path, help='Optional real binary instead of the source handler adapter')
    ap.add_argument('--python-source', type=Path, default=ROOT.parent/'skill_lsp/skill_lsp.py')
    ap.add_argument('--rust-binary', type=Path, default=ROOT/'target/release/skill-lsp')
    ap.add_argument('--benchmark', action='store_true')
    ap.add_argument('--fixture-dir', type=Path, default=ROOT/'examples/SKILL')
    ap.add_argument('--all-api-hovers', type=Path)
    ap.add_argument('--fuzz-cases',type=int,default=12)
    args = ap.parse_args()
    python_command=([str(args.python_binary.resolve())] if args.python_binary else
                    [sys.executable,str(ROOT/'scripts/python_reference.py'),'--source',str(args.python_source.resolve())])
    commands = [python_command+['--fnd-dir',str(args.fixture_dir)],
                [str(args.rust_binary.resolve()),'--fnd-dir',str(args.fixture_dir)]]
    clients = []
    checks = 0
    try:
        for command in commands:
            client, init = connect(command)
            clients.append(client)
            assert init['capabilities']['semanticTokensProvider']['legend']['tokenTypes'][1:11] == [f'keyword{i}' for i in range(1,11)]
        uri = (ROOT/'examples/parity.il').as_uri()
        cases = [
            'procedure(myFn(cell)\n dbSynthetic(cell "name")\n)\n; myFn\n"myFn"\nmyFn(nil)\n',
            'defvar variable 1\nsetq variable 2\nprocedure(dbSynthetic(x) x)\ndbSynthetic\n',
            '; procedure(fake() nil)\n"procedure(hidden() t)"\nfake hidden\n',
            '(dbSynthetic nil, "name", ?width 3\n',
            'dbSynthetic(nil "name"\n',
            'dbSynthetic(\n nil\n "name"\n',
            '(if nil, t\n(unknown one, two\n',
            ')\n(\n"unterminated (\n/* ( */\n',
            'procedure(f()\r\n  dbSynthetic(nil)\r\n)\r\n',
            '"😀" dbSynthetic(nil)\n?width foo? foo* let* 1-2 1e-2 0xFF .25\n',
            '', '\n', '(', 'dbSynthetic',
        ]
        rng=random.Random(26021)
        atoms=['procedure(alpha(x) x)','setq alpha 2','defvar value 1','(defun beta (x) x)',
               'mprocedure(gamma() nil)','; procedure(fake() nil)','"alpha beta"',
               'foo? foo* let* ?width','"escaped \\\" quote"','1e-2 -0xFF .25',
               '/* (alpha) */','(dbSynthetic nil, "name"','dbSynthetic(nil, t)',
               ')','(','\tvalue = 2','"unfinished\\','"😀" alpha']
        cases.extend('\n'.join(rng.choices(atoms,k=6)) for _ in range(args.fuzz_cases))
        for version, text in enumerate(cases,1):
            for client in clients:
                client.send('textDocument/didOpen', {'textDocument': {
                    'uri':uri, 'languageId':'skill', 'version':version, 'text':text}})
            for method in ('textDocument/documentSymbol','textDocument/semanticTokens/full','textDocument/formatting'):
                results=[query(c,method,uri) for c in clients]
                assert results[0] == results[1], (version,method,results)
                checks+=1
            for line, content in enumerate(text.splitlines()):
                for column in range(len(content)+1):
                    for method in ('textDocument/hover','textDocument/definition','textDocument/references','textDocument/signatureHelp','textDocument/completion'):
                        results=[query(c,method,uri,line,column) for c in clients]
                        compare(results,(version,method,line,column))
                        checks+=1
            diagnostics=[]
            for client in clients:
                notifications=[n['params']['diagnostics'] for n in client.notifications
                               if n.get('method')=='textDocument/publishDiagnostics']
                diagnostics.append(notifications[-1])
            assert diagnostics[0] == diagnostics[1], (version,'diagnostics',diagnostics)
            checks+=1
        # Full-text changes, including multiple changes and decreasing versions.
        for version in (10,9):
            for client in clients:
                client.send('textDocument/didChange', {'textDocument':{'uri':uri,'version':version},
                    'contentChanges':[{'text':'discarded'},{'text':'(dbSynthetic nil, "name"'}]})
            for method in ('textDocument/signatureHelp','textDocument/completion'):
                results=[query(c,method,uri,0,23) for c in clients]
                assert results[0]==results[1], (method,results)
                checks+=1
        for client in clients:
            client.send('textDocument/didClose', {'textDocument':{'uri':uri}})
        for method in ('textDocument/hover','textDocument/completion','textDocument/documentSymbol','textDocument/references','textDocument/formatting','textDocument/semanticTokens/full'):
            results=[query(c,method,uri) for c in clients]
            assert results[0]==results[1], (method,results)
            checks+=1
        if args.all_api_hovers:
            names=[e['name'] for e in json.loads(args.all_api_hovers.read_text())]
            text='\n'.join(names)
            for client in clients:
                client.send('textDocument/didOpen', {'textDocument':{'uri':uri,'languageId':'skill','version':50,'text':text}})
            for line,name in enumerate(names):
                for method in ('textDocument/hover','textDocument/completion'):
                    results=[query(c,method,uri,line,len(name)) for c in clients]
                    assert results[0]==results[1], (method,name,results)
                    checks+=1
        if args.benchmark:
            text='procedure(myFn(cell)\n dbSynthetic(cell "name")\n)\n'*1000
            for client in clients:
                client.send('textDocument/didOpen', {'textDocument':{'uri':uri,'languageId':'skill','version':100,'text':text}})
                query(client,'textDocument/hover',uri,1,5)
            for method in ('textDocument/hover','textDocument/completion','textDocument/semanticTokens/full'):
                timings=[]
                for client in clients:
                    samples=[]
                    for _ in range(30):
                        start=time.perf_counter()
                        query(client,method,uri,1,5)
                        samples.append((time.perf_counter()-start)*1000)
                    timings.append(statistics.median(samples))
                print(f'{method}: Python {timings[0]:.3f} ms; Rust {timings[1]:.3f} ms; {timings[0]/timings[1]:.2f}x (median of 30, 3000 lines)')
        for client in clients:
            client.close()
        print(f'PASS: {checks} Python/Rust LSP response comparisons (synthetic data only)')
    finally:
        for client in clients:
            if client.process.poll() is None:
                client.process.kill()
                client.process.wait()

if __name__ == '__main__':
    main()
