#!/usr/bin/env python3
"""Release gates must reject a wrong source or corrupt package before publishing."""
import hashlib
import json
import os
from pathlib import Path
import unittest
from unittest.mock import patch

import publish


class ReleaseGates(unittest.TestCase):
    def attempt(self, wrong_source=False, corrupt=False):
        calls = []
        commit = 'a' * 40
        info = dict(headSha='b' * 40 if wrong_source else commit, status='completed', conclusion='success', workflowName='Readus development')

        def gh(*args):
            calls.append(args)
            if args[:2] == ('run', 'view'):
                return json.dumps(info)
            if args[:2] == ('run', 'download'):
                stage = Path(args[args.index('--dir') + 1])
                files = {'readus-dev.zip': b'app archive', 'readus-dev-macos-arm64.dmg': b'disk image'}
                for name, data in files.items():
                    (stage / name).write_bytes(data)
                (stage / 'SHA256SUMS.txt').write_text(''.join(f'{hashlib.sha256(data).hexdigest()}  {name}\n' for name, data in files.items()))
                (stage / 'metadata.json').write_text(json.dumps(dict(commit=commit, repository='owner/readus-md', run_id='123', architecture='arm64', devtools=True, sha256=hashlib.sha256(files['readus-dev.zip']).hexdigest())))
                if corrupt:
                    (stage / 'readus-dev-macos-arm64.dmg').write_bytes(b'corrupt')
            return ''

        with patch.dict(os.environ, dict(DEVELOPMENT_RUN='123', GITHUB_SHA=commit, GITHUB_REPOSITORY='owner/readus-md')), patch.object(publish, 'gh', gh), patch.object(publish.subprocess, 'run'):
            if wrong_source or corrupt:
                with self.assertRaises(ValueError):
                    publish.main()
                self.assertFalse(any(c[:2] == ('release', 'create') for c in calls))
            else:
                publish.main()
                release = next(c for c in calls if c[:2] == ('release', 'create'))
                self.assertIn('--prerelease', release)
                self.assertIn('--latest=false', release)
                self.assertEqual(release[release.index('--target') + 1], commit)

    def test_wrong_source_never_publishes(self):
        self.attempt(wrong_source=True)

    def test_corrupt_dmg_never_publishes(self):
        self.attempt(corrupt=True)

    def test_valid_package_is_only_a_prerelease(self):
        self.attempt()


if __name__ == '__main__':
    unittest.main()
