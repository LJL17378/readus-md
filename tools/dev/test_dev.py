"""Replacement is destructive: verify failed checks preserve the current app."""
import hashlib
import importlib.util
import json
from pathlib import Path
import platform
import tempfile
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location('readus_dev', Path(__file__).with_name('dev.py'))
dev = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dev)


class DownloadTests(unittest.TestCase):
    def exercise(self, invalid=False):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            current = root / 'current'
            current.mkdir()
            (current / 'old-app').write_text('keep until verified')
            info = dict(databaseId=12, headSha='abc', status='completed', conclusion='success', workflowName='Readus development')

            def gh(*args):
                destination = Path(args[-1])
                archive = destination / 'readus-dev.zip'
                with zipfile.ZipFile(archive, 'w') as z:
                    z.writestr('Readus.md Dev.app/Contents/MacOS/quiet_md', 'new executable')
                digest = hashlib.sha256(archive.read_bytes()).hexdigest()
                meta = dict(commit='abc', repository='owner/fork', run_id='12', architecture=platform.machine(),
                    devtools=True, sha256='wrong' if invalid else digest)
                (destination / 'metadata.json').write_text(json.dumps(meta))

            def run(*args, **kw):
                if args[0] == 'ditto':
                    with zipfile.ZipFile(args[3]) as z:
                        z.extractall(args[4])

            with patch.multiple(dev, DEV=root, CURRENT=current), patch.object(dev, 'selected_run', return_value=info), \
                    patch.object(dev, 'sha', return_value='abc'), patch.object(dev, 'repo', return_value='owner/fork'), \
                    patch.object(dev, 'gh', side_effect=gh), patch.object(dev, 'run', side_effect=run), patch.object(dev, 'stop'):
                if invalid:
                    with self.assertRaisesRegex(ValueError, 'checksum'):
                        dev.pull(12)
                    self.assertEqual((current / 'old-app').read_text(), 'keep until verified')
                else:
                    dev.pull(12)
                    self.assertTrue((current / 'Readus.md Dev.app/Contents/MacOS/quiet_md').is_file())
                    self.assertFalse((current / 'old-app').exists())
                    self.assertEqual({p.name for p in root.iterdir()}, {'current'})

    def test_bad_checksum_leaves_current_app_untouched(self):
        self.exercise(invalid=True)

    def test_success_removes_old_app_and_download(self):
        self.exercise()


if __name__ == '__main__':
    unittest.main()
