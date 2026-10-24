#!/usr/bin/env python3
"""Run the original Python handlers without installing pygls.

This test adapter supplies only protocol value classes, decorators and stdio
framing. Document analysis, Finder parsing and all responses come unmodified
from the user-supplied Python source. It is not a pygls transport test.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import sys
import types as module_types

class Value:
    def __init__(self, **kwargs):
        self.__dict__.update(kwargs)

class ProtocolTypes:
    CompletionItemKind=Value(Keyword=14,Function=3,Variable=6)
    SymbolKind=Value(Function=12)
    DiagnosticSeverity=Value(Error=1)
    MarkupKind=Value(Markdown='markdown')
    def __getattr__(self, name):
        return name if name.isupper() else Value

class Server:
    def __init__(self,*args):
        self.diagnostics=[]
    def feature(self,*args):
        return lambda fn:fn
    def publish_diagnostics(self,uri,diagnostics):
        self.diagnostics.append({'uri':uri,'diagnostics':diagnostics})

def objectify(value):
    if isinstance(value,dict):
        return Value(**{re.sub(r'(?<!^)(?=[A-Z])','_',k).lower():objectify(v) for k,v in value.items()})
    if isinstance(value,list):
        return [objectify(v) for v in value]
    return value

def serialize(value):
    if isinstance(value,dict):
        return {k:serialize(v) for k,v in value.items()}
    if isinstance(value,Value):
        return {re.sub(r'_([a-z])',lambda m:m[1].upper(),k):serialize(v) for k,v in vars(value).items()}
    if isinstance(value,list):
        return [serialize(v) for v in value]
    return value

def send(value):
    payload=json.dumps(serialize(value)).encode()
    sys.stdout.buffer.write(f'Content-Length: {len(payload)}\r\n\r\n'.encode()+payload)
    sys.stdout.buffer.flush()

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--source',type=Path,required=True)
    ap.add_argument('--fnd-dir',type=Path,required=True)
    args=ap.parse_args()
    pygls=module_types.ModuleType('pygls')
    pygls_server=module_types.ModuleType('pygls.server')
    pygls_server.LanguageServer=Server
    lsprotocol=module_types.ModuleType('lsprotocol')
    lsprotocol.types=ProtocolTypes()
    sys.modules.update({'pygls':pygls,'pygls.server':pygls_server,'lsprotocol':lsprotocol})
    sys.path.insert(0,str(args.source.resolve().parent))
    spec=importlib.util.spec_from_file_location('python_skill_reference',args.source)
    original=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(original)
    original._init(args.fnd_dir)
    handlers={
        'textDocument/didOpen':original.did_open,
        'textDocument/didChange':original.did_change,
        'textDocument/didClose':original.did_close,
        'textDocument/hover':original.hover,
        'textDocument/completion':original.completion,
        'textDocument/definition':original.definition,
        'textDocument/references':original.references,
        'textDocument/documentSymbol':original.document_symbol,
        'textDocument/semanticTokens/full':original.semantic_tokens_full,
        'textDocument/formatting':original.formatting,
        'textDocument/signatureHelp':original.signature_help,
    }
    while True:
        headers={}
        while True:
            line=sys.stdin.buffer.readline()
            if not line:return
            if line==b'\r\n':break
            key,value=line.decode().split(':',1)
            headers[key.lower()]=value.strip()
        message=json.loads(sys.stdin.buffer.read(int(headers['content-length'])))
        method=message['method']
        if method=='exit':return
        if method=='initialize':
            result={'capabilities':{'semanticTokensProvider':{'legend':{'tokenTypes':original._TOKEN_TYPES}}}}
        elif method in ('initialized','shutdown'):
            result=None
        else:
            result=handlers[method](original.server,objectify(message['params']))
        for diagnostics in original.server.diagnostics:
            send({'jsonrpc':'2.0','method':'textDocument/publishDiagnostics','params':diagnostics})
        original.server.diagnostics.clear()
        if 'id' in message:
            send({'jsonrpc':'2.0','id':message['id'],'result':result})

if __name__=='__main__':main()
