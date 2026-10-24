#!/usr/bin/env python3
"""Generate invented Finder records and compare the real Python parser to Rust."""
import argparse
import importlib.util
import json
from pathlib import Path
import random
import shutil
import subprocess
import sys
import tempfile
from smoke import ROOT

def quoted(s):
    return '"'+s.replace('\\','\\\\').replace('"','\\"')+'"'

def record(name,signature,description):
    return '('+' '.join(map(quoted,(name,signature,description)))+')\n'

def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--binary',type=Path,default=ROOT/'target/release/skill-lsp')
    args=ap.parse_args()
    spec=importlib.util.spec_from_file_location('reference_fnd',ROOT.parent/'skill_lsp/fnd_parser.py')
    parser=importlib.util.module_from_spec(spec)
    sys.modules[spec.name]=parser
    spec.loader.exec_module(parser)
    rng=random.Random(417)
    with tempfile.TemporaryDirectory(prefix='skill-lsp-synthetic-', dir=ROOT/'target') as tmp:
        fixture=Path(tmp)/'SKILL'
        shutil.copytree(ROOT/'examples/SKILL',fixture)
        signatures=[
            ' t_name\n [ x_width {0..10} ]',
            ' [\n ?purpose\n t_purpose\n ]',
            ' [ ?layer l_layer ]\n g_required',
            ' [\n [ t_nested ]\n g_tail\n ]\n x_last',
            ' txf_number\n &p_pointer\n z_unknown',
            ' [ ?some_key t_some_key ]\n [ ?width x_width ]',
            ' t_first [ x_next ] g_end\n [\n ]',
            ' t_input {annotation [constraint]}\n t_next',
            ' t_input {unclosed\n t_next',
            ' t_input\n t_second)\n (skip this line)',
            '',
        ]
        descriptions=[
            'Invented description with (parentheses) and "quotes".',
            ' '.join(['Invented text for wrapping.']*12),
            'word '*12+'hyphenated-description suffix',
            'word '*12+'alpha--beta suffix',
            'a'*180,
            'First\tcolumn  second\nline.\tEnd.',
            '  leading spaces '+('a '*60)+' trailing spaces  ',
            'λ Unicode description with invented metadata 😀.',
        ]
        paths=['Core_SKILL/generated.fnd','Core_SKILL/nested/deeper.fnd','misc/generated.fnd',
               'Pcells/skpcellref.fnd','OCEAN/oceanref.fnd','Schematics/skcompref.fnd']
        for i in range(80):
            path=fixture/paths[i%len(paths)]
            path.parent.mkdir(parents=True,exist_ok=True)
            name=rng.choice(['db','str','sample','other','rod','plain'])+f'Invented{i}'
            signature=name+'(\n'+signatures[i%len(signatures)]+'\n)\n=> '+rng.choice(['t / nil','d_result / nil','void','txf_result','list','nil','T/NIL'])
            with path.open('a') as stream:
                stream.write(record(name,signature,descriptions[i%len(descriptions)]))
        # Python scans quoted descriptions for core prefixes, too.
        with (fixture/'Core_SKILL/generated.fnd').open('a') as stream:
            stream.write(record('hint','hint(\n)\n=> nil','unrelatedHint'))
            stream.write('(\"unfinished\" \"not closed')
        (fixture/'misc/tie.fnd').write_text(record('collision','collision(\n t_child\n)','Nested copy'))
        (fixture/'root.fnd').write_text(record('collision','collision(\n t_root\n)','Root copy'))
        (fixture/'misc/description-prefix.fnd').write_text(record('unrelatedFunction','unrelatedFunction(\n)','Invented prefix propagation'))
        with (fixture/'Core_SKILL/nested/deeper.fnd').open('a') as stream:
            stream.write(record('nestedplain','nestedplain(\n)','Nested file is not immediate core'))
        expected=parser.load_fnd_dir(fixture)
        output=Path(tmp)/'api.json'
        subprocess.run([str(args.binary.resolve()),'index',str(fixture),str(output)],check=True)
        actual={e['name']:e for e in json.loads(output.read_text())}
        want={k:e.to_dict() for k,e in expected.items()}
        assert actual==want, [(k,actual.get(k),want.get(k)) for k in sorted(actual.keys()|want.keys()) if actual.get(k)!=want.get(k)][:3]
        print(f'PASS: {len(actual)} generated/synthetic records match original Python parser metadata')
        # The ordinary LSP suite also asks for the generated descriptions below.
        subprocess.run([sys.executable,str(ROOT/'scripts/parity.py'),'--rust-binary',str(args.binary.resolve()),
                        '--fixture-dir',str(fixture),'--all-api-hovers',str(output)],check=True)

if __name__=='__main__':main()
