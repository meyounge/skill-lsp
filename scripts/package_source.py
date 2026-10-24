#!/usr/bin/env python3
"""Export only explicitly reviewed public files. Never walk the Finder library."""
import argparse
import json
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SYNTHETIC = {
    'examples/SKILL/Core_SKILL/synthetic.fnd',
    'examples/SKILL/Custom_Layout/sklayoutref.fnd',
}


def files():
    names = json.loads((ROOT/'PUBLIC_FILES.json').read_text())
    assert len(names) == len(set(names)), 'Duplicate public manifest entry'
    for name in names:
        path = ROOT/name
        assert not path.is_symlink(), f'Refusing symlink: {name}'
        assert path.resolve().is_relative_to(ROOT), f'Path outside project: {name}'
        assert path.is_file(), f'Missing public file: {name}'
        assert not any(part in ('node_modules', 'target', '__pycache__') for part in Path(name).parts), name
        assert not name.endswith('.api.json'), name
        assert not name.endswith('.fnd') or name in SYNTHETIC, f'Non-synthetic Finder file: {name}'
        yield name, path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT/'target/skill-lsp-public-source.zip')
    parser.add_argument('--check', action='store_true', help='Validate the manifest without creating an archive')
    args = parser.parse_args()
    public = list(files())
    if args.check:
        print(f'PASS: {len(public)} allowlisted public files; no library traversal')
        return
    output = args.output.resolve()
    assert output.is_relative_to(ROOT), 'Write the export inside this project'
    assert not output.exists(), f'Output already exists: {output}; choose a new name'
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
        for name, path in public:
            archive.write(path, f'skill-lsp/{name}')
    print(f'Exported {len(public)} public files to {output}')


if __name__ == '__main__':
    main()
