#!/usr/bin/env python3
# Minifies a page of the website: CSS and scripts through esbuild, JSON-LD compacted, whitespace between tags
# removed (<pre> kept). The repository keeps the readable sources; the Pages workflow publishes the minified ones.
# Usage: minify.py page.html out.html (needs Node for npx)
import json
import re
import subprocess
import sys

ESBUILD = ['npx', '--yes', 'esbuild@0.24.0', '--minify', '--target=safari16']
LDJSON = '<script type="application/ld+json">'


def esbuild(code, loader):
    return subprocess.run(ESBUILD + ['--loader=' + loader], input=code, capture_output=True, text=True, check=True).stdout.strip()


def minify(part):
    if part.startswith('<style>'):
        return '<style>' + esbuild(part[7:-8], 'css') + '</style>'
    if part.startswith('<script>'):
        return '<script>' + esbuild(part[8:-9], 'js') + '</script>'
    if part.startswith(LDJSON):
        return LDJSON + json.dumps(json.loads(part[len(LDJSON):-9]), separators=(',', ':'), ensure_ascii=False) + '</script>'
    if part.startswith('<pre'):
        return part
    return re.sub(r'>\s+<', '><', part)


src, dst = sys.argv[1:]
source = open(src).read()
parts = re.split(r'(<style>.*?</style>|<script>.*?</script>|<script type="application/ld\+json">.*?</script>|<pre[\s>].*?</pre>)', source, flags=re.S)
open(dst, 'w').write(''.join(minify(p) for p in parts))
