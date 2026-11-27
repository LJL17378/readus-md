#!/usr/bin/env python3
"""Publish only the checked, exact-source development artifact as a prerelease."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile


def gh(*args):
    return subprocess.check_output(['gh', *args], text=True)


def main():
    run_id = os.environ['DEVELOPMENT_RUN']
    if not re.fullmatch(r'[0-9]+', run_id):
        raise ValueError('Development run must be a numeric run ID.')
    commit, repo = os.environ['GITHUB_SHA'], os.environ['GITHUB_REPOSITORY']
    info = json.loads(gh('run', 'view', run_id, '--repo', repo, '--json', 'headSha,status,conclusion,workflowName'))
    if (info['headSha'], info['status'], info['conclusion'], info['workflowName']) != (commit, 'completed', 'success', 'Readus development'):
        raise ValueError('A successful Readus development run for this exact commit is required.')
    with tempfile.TemporaryDirectory(prefix='readus-publish-') as tmp:
        stage = Path(tmp)
        gh('run', 'download', run_id, '--repo', repo, '--name', 'readus-dev', '--dir', str(stage))
        meta = json.loads((stage / 'metadata.json').read_text())
        if (meta['commit'], meta['repository'], str(meta['run_id']), meta['architecture'], meta['devtools']) != (commit, repo, run_id, 'arm64', True):
            raise ValueError('Artifact provenance does not match the selected build.')
        files = ['readus-dev.zip', 'readus-dev-macos-arm64.dmg']
        sums = {}
        for line in (stage / 'SHA256SUMS.txt').read_text().splitlines():
            digest, name = line.split('  ', 1)
            sums[name] = digest
        for name in files:
            with (stage / name).open('rb') as f:
                digest = hashlib.file_digest(f, 'sha256').hexdigest()
            if digest != sums.get(name) or (name.endswith('.zip') and digest != meta['sha256']):
                raise ValueError('Artifact checksum mismatch.')
        unpacked = stage / 'unpacked'
        subprocess.run(['ditto', '-x', '-k', str(stage / files[0]), str(unpacked)], check=True)
        subprocess.run(['codesign', '--verify', '--deep', '--strict', str(unpacked / 'Readus.md Dev.app')], check=True)
        tag = f'readus-test-{commit[:7]}'
        notes = stage / 'release-notes.md'
        notes.write_text(Path('docs/releases/readus-test.md').read_text() +
            f'\n\nSource: `{commit}` · [successful build](https://github.com/{repo}/actions/runs/{run_id}).\n')
        # An existing tag/release is never silently replaced.
        gh('release', 'create', tag, *(str(stage / name) for name in files + ['SHA256SUMS.txt', 'metadata.json']),
            '--repo', repo, '--target', commit, '--prerelease', '--latest=false',
            '--title', f'Readus.md Test · {commit[:7]}', '--notes-file', str(notes))
        print(f'https://github.com/{repo}/releases/tag/{tag}')


if __name__ == '__main__':
    main()
