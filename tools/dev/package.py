#!/usr/bin/env python3
"""Package the CI-built executable, without a DMG or local compilation."""
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess

root = Path(__file__).resolve().parents[2]
out = root / 'dist-dev'
app = out / 'Readus.md Dev.app'
contents = app / 'Contents'
(contents / 'MacOS').mkdir(parents=True, exist_ok=True)
(contents / 'Resources').mkdir(exist_ok=True)
shutil.copy2(root / 'target/release/quiet_md', contents / 'MacOS/quiet_md')
shutil.copy2(root / 'assets/branding/app-icon-dev.png', contents / 'Resources/app-icon.png')
version = next(line.split('"')[1] for line in (root / 'Cargo.toml').read_text().splitlines() if line.startswith('version = '))
with (contents / 'Info.plist').open('wb') as f:
    plistlib.dump(dict(CFBundleName='Readus.md Dev', CFBundleDisplayName='Readus.md Dev',
        CFBundleIdentifier='local.readusmd.dev', CFBundleExecutable='quiet_md', CFBundlePackageType='APPL',
        CFBundleShortVersionString=version, NSHighResolutionCapable=True, LSMinimumSystemVersion='13.0'), f)
subprocess.run(['codesign', '--force', '--sign', '-', str(app)], check=True)
archive = out / 'readus-dev.zip'
subprocess.run(['ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', str(app), str(archive)], check=True)
meta = dict(commit=os.environ['GITHUB_SHA'], repository=os.environ['GITHUB_REPOSITORY'],
    run_id=os.environ['GITHUB_RUN_ID'], architecture=platform.machine(), version=version,
    devtools=True, sha256=hashlib.file_digest(archive.open('rb'), 'sha256').hexdigest())
(out / 'metadata.json').write_text(json.dumps(meta, indent=2) + '\n')
shutil.rmtree(app)
