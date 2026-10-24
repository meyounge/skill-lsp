#!/usr/bin/env python3
"""Exercise computed car/cdr selectors over LSP, with invented JSON data only."""
import argparse
import json
import os
from pathlib import Path
import tempfile
from smoke import Client, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT/'target/release'/
                        ('skill-lsp.exe' if os.name == 'nt' else 'skill-lsp'))
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='car-cdr-test-', dir=ROOT/'target') as tmp:
        database = Path(tmp)/'synthetic.api.json'
        database.write_text(json.dumps([
            dict(name=name, signature_raw=f'{name}(\n g_value\n)',
                 description='Invented Finder description that should lose to computed selectors.',
                 params=[], return_type=None, source_file='invented.json', keyword_category=6)
            for name in ('caadar', 'cadrExtra')
        ]))
        client = Client([str(args.binary.resolve()), '--database', str(database)])
        try:
            result = client.send('initialize', {'processId': None, 'capabilities': {}}, request=True)
            legend = result['capabilities']['semanticTokensProvider']['legend']['tokenTypes']
            assert legend[-2:] == ['kwarg', 'literal'] and len(legend) == 19
            client.send('initialized', {})
            uri = (ROOT/'examples/selector-smoke.il').as_uri()
            lines = [
                'procedure(caadar(value) value)',
                'caadar(lst) cadr(lst) caar(lst) cadar(lst) cddr(lst) cdar(lst)',
                '"caadar" ; cadr',
                'carpet cadrExtra Caadar t nil',
                'car(lst) cdr(lst)',
                'procedure(bbEdit(bBox @key x1 y1 x2 y2) nil)',
                'procedure(bbIntersect(bBox @rest bBoxes) nil)',
                'procedure(drcRound(val @optional (snap 0.005)) nil)',
                '"@key @rest @optional" ; @key',
            ]
            client.send('textDocument/didOpen', {'textDocument': {
                'uri': uri, 'languageId': 'skill', 'version': 1, 'text': '\n'.join(lines)}})

            def at(method, line, column):
                return client.send(method, {'textDocument': {'uri': uri},
                    'position': {'line': line, 'character': column}}, request=True)

            for name, path in [('caadar', 'lst[1][2][1]'), ('cadr', 'lst[2]'),
                               ('caar', 'lst[1][1]'), ('cadar', 'lst[1][2]'),
                               ('cddr', 'lst[3..]'), ('cdar', 'lst[1][2..]')]:
                column = lines[1].index(name+'(')+1
                hover = at('textDocument/hover', 1, column)['contents']['value']
                assert f'`{path}`' in hover, (name, hover)
                assert 'Invented Finder' not in hover
                assert at('textDocument/definition', 1, column) is None
            hover = at('textDocument/hover', 1, 1)['contents']['value']
            assert '((- (x -) -) -)' in hover
            assert 'car(car(cdr(car(lst))))' in hover
            assert at('textDocument/hover', 3, 1) is None  # carpet is not an accessor
            ordinary = at('textDocument/hover', 3, lines[3].index('cadrExtra')+1)
            assert 'Invented Finder' in ordinary['contents']['value']
            complete = at('textDocument/completion', 1, len('caadar'))
            api_item = next(x for x in complete['items'] if x['label'] == 'caadar' and 'documentation' in x)
            assert 'lst[1][2][1]' in api_item['documentation']['value']
            data = client.send('textDocument/semanticTokens/full', {'textDocument': {'uri': uri}}, request=True)['data']
            positions = {}
            lengths = {}
            line = column = 0
            for i in range(0, len(data), 5):
                delta, start, length, kind, _ = data[i:i+5]
                line += delta
                column = start if delta else column+start
                positions[(line, column)] = legend[kind]
                lengths[(line, column)] = length
            assert positions[(0, len('procedure('))] == 'keyword2'  # user declaration loses
            for name in ('caadar', 'cadr', 'caar', 'cadar', 'cddr', 'cdar'):
                assert positions[(1, lines[1].index(name+'('))] == 'keyword2'
            assert positions[(2, 0)] == 'string'
            assert not any(line == 2 and kind == 'keyword2' for (line, _), kind in positions.items())
            assert positions[(3, lines[3].index('cadrExtra'))] == 'keyword6'
            assert positions[(4, 0)] == positions[(4, len('car(lst) '))] == 'keyword2'
            for line, name in [(5, '@key'), (6, '@rest'), (7, '@optional')]:
                assert positions[(line, lines[line].index(name))] == 'keyword7'
                assert lengths[(line, lines[line].index(name))] == len(name)
            assert not any(line == 8 and kind == 'keyword7' for (line, _), kind in positions.items())
            client.close()
        finally:
            if client.process.poll() is None:
                client.process.kill()
                client.process.wait()
    print('PASS: car/cdr hover/keyword2 precedence and @key/@rest/@optional keyword7 tokens')


if __name__ == '__main__':
    main()
