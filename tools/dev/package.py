#!/usr/bin/env python3
"""Package the CI-built executable as a test ZIP and drag-to-install DMG."""
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess
import tempfile

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
        CFBundleIconFile="app-icon.icns", CFBundleShortVersionString=version, NSHighResolutionCapable=True, LSMinimumSystemVersion='13.0'), f)
icons = out / 'app-icon.iconset'
icons.mkdir(exist_ok=True)
for size in (16, 32, 128, 256, 512):
    for scale in (1, 2):
        suffix = '@2x' if scale == 2 else ''
        subprocess.run(['sips', '-z', str(size * scale), str(size * scale),
            str(root / 'assets/branding/app-icon-dev.png'), '--out', str(icons / f'icon_{size}x{size}{suffix}.png')], check=True, stdout=subprocess.DEVNULL)
subprocess.run(['iconutil', '-c', 'icns', str(icons), '-o', str(contents / 'Resources/app-icon.icns')], check=True)
shutil.rmtree(icons)
subprocess.run(['codesign', '--force', '--sign', '-', str(app)], check=True)
archive = out / 'readus-dev.zip'
subprocess.run(['ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', str(app), str(archive)], check=True)
meta = dict(commit=os.environ['GITHUB_SHA'], repository=os.environ['GITHUB_REPOSITORY'],
    run_id=os.environ['GITHUB_RUN_ID'], architecture=platform.machine(), version=version,
    devtools=True, sha256=hashlib.file_digest(archive.open('rb'), 'sha256').hexdigest())
(out / 'metadata.json').write_text(json.dumps(meta, indent=2) + '\n')
image = out / 'readus-dev-macos-arm64.dmg'
with tempfile.TemporaryDirectory(prefix='readus-dmg-') as tmp:
    stage = Path(tmp)
    shutil.copytree(app, stage / app.name)
    (stage / 'Applications').symlink_to('/Applications')
    (stage / 'READ ME.txt').write_text(
        'Readus.md Test — macOS 13+, Apple Silicon\n\n'
        'Drag Readus.md Dev.app to Applications, then launch it there.\n'
        'This test build is ad-hoc signed, not Apple-notarized. macOS may require approval in Privacy & Security.\n'
        'Double-click launches use a separate library under ~/Library/Application Support/Readus.md Test.\n'
        '测试版：拖入 Applications 后启动；默认使用独立文档库，不自动打开 Quiet.md 的文档库。\n')
    subprocess.run(['hdiutil', 'create', '-volname', 'Readus.md Test', '-srcfolder', str(stage),
        '-ov', '-format', 'UDZO', str(image)], check=True)
(out / 'SHA256SUMS.txt').write_text(''.join(
    f'{hashlib.file_digest(p.open("rb"), "sha256").hexdigest()}  {p.name}\n' for p in (archive, image)))
shutil.rmtree(app)
