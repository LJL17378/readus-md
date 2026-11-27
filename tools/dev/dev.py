#!/usr/bin/env python3
"""Cloud builds, one local app, isolated runtime data. No local Rust commands."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import zipfile

ROOT = Path(__file__).resolve().parents[2]
DEV = ROOT / '.dev'
CURRENT = DEV / 'current'
BIN = CURRENT / 'Readus.md Dev.app/Contents/MacOS/quiet_md'


def run(*args, capture=False, **kw):
    output = kw.pop('stdout', subprocess.PIPE if capture else None)
    return subprocess.run(args, cwd=ROOT, check=True, text=True, stdout=output, **kw).stdout


def gh(*args):
    return run('gh', *map(str, args), capture=True)


def repo():
    return gh('repo', 'view', '--json', 'nameWithOwner', '--jq', '.nameWithOwner').strip()


def sha():
    return run('git', 'rev-parse', 'HEAD', capture=True).strip()


def selected_run(run_id=None):
    if run_id is None:
        branch = run('git', 'branch', '--show-current', capture=True).strip()
        runs = json.loads(gh('run', 'list', '--workflow', 'dev-build.yml', '--branch', branch,
            '--commit', sha(), '--limit', '1', '--json', 'databaseId'))
        if not runs:
            raise ValueError('No development run for HEAD. Commit and push the codex/ branch first.')
        run_id = runs[0]['databaseId']
    return json.loads(gh('run', 'view', run_id, '--json', 'databaseId,headSha,status,conclusion,workflowName'))


def stop():
    pidfile = DEV / 'pid'
    if not pidfile.exists():
        return
    pid = int(pidfile.read_text())
    ps = subprocess.run(['ps', '-p', str(pid), '-o', 'command='], text=True, capture_output=True)
    if ps.returncode == 0:
        command = ps.stdout.strip()
        if command != str(BIN) and not command.startswith(str(BIN) + ' '):
            raise ValueError('Recorded PID belongs to another process; refusing to stop it.')
        os.kill(pid, signal.SIGTERM)
        for _ in range(50):
            if subprocess.run(['kill', '-0', str(pid)], capture_output=True).returncode:
                break
            time.sleep(0.1)
        else:
            raise ValueError('Development app did not stop; keeping the current build untouched.')
    pidfile.unlink(missing_ok=True)


def pull(run_id=None):
    info = selected_run(run_id)
    if info['workflowName'] != 'Readus development':
        raise ValueError('Unexpected workflow; current app was not changed.')
    if info['status'] != 'completed' or info['conclusion'] != 'success':
        raise ValueError('The entire development run must succeed before downloading.')
    if info['headSha'] != sha():
        raise ValueError('Run does not match HEAD; check out its exact commit before testing it.')
    DEV.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='download-', dir=DEV) as tmp:
        stage = Path(tmp)
        gh('run', 'download', info['databaseId'], '--name', 'readus-dev', '--dir', stage)
        meta = json.loads((stage / 'metadata.json').read_text())
        if (meta['commit'], meta['repository'], str(meta['run_id'])) != (sha(), repo(), str(info['databaseId'])):
            raise ValueError('Artifact repository, commit or run ID does not match.')
        if meta['architecture'] != platform.machine() or not meta.get('devtools'):
            raise ValueError('Artifact architecture or devtools configuration does not match.')
        archive = stage / 'readus-dev.zip'
        with archive.open('rb') as f:
            if hashlib.file_digest(f, 'sha256').hexdigest() != meta['sha256']:
                raise ValueError('Artifact checksum mismatch; current app was not changed.')
        with zipfile.ZipFile(archive) as z:
            for entry in z.infolist():
                path = Path(entry.filename)
                if path.is_absolute() or '..' in path.parts or (entry.external_attr >> 16) & 0o170000 == 0o120000:
                    raise ValueError('Unsafe archive entry.')
        unpacked = stage / 'unpacked'
        run('ditto', '-x', '-k', str(archive), str(unpacked))
        app = unpacked / 'Readus.md Dev.app'
        run('codesign', '--verify', '--deep', '--strict', str(app))
        if not (app / 'Contents/MacOS/quiet_md').is_file():
            raise ValueError('Development executable is missing.')
        shutil.copy2(stage / 'metadata.json', unpacked / 'metadata.json')
        stop()
        old = DEV / 'replaced'
        if old.exists():
            shutil.rmtree(old)
        if CURRENT.exists():
            CURRENT.rename(old)
        try:
            unpacked.rename(CURRENT)
        except OSError:
            if old.exists():
                old.rename(CURRENT)
            raise
        # The user requests only the newest test version; no previous app or zip is retained.
        if old.exists():
            shutil.rmtree(old)
    print(json.dumps(meta, indent=2))


def launch():
    if not BIN.is_file():
        raise ValueError('No verified app. Run dev.sh pull first.')
    stop()
    for name in ('library', 'logs'):
        (DEV / name).mkdir(parents=True, exist_ok=True)
    env = {**os.environ, 'QUIET_MD_HOME': str(DEV / 'library'),
        'QUIET_MD_LOG_DIR': str(DEV / 'logs'), 'DO_NOT_TRACK': '1', 'QUIET_NO_TELEMETRY': '1'}
    with (DEV / 'logs/console.log').open('w') as f:
        p = subprocess.Popen([str(BIN)], cwd=ROOT, env=env, stdout=f, stderr=subprocess.STDOUT, start_new_session=True)
    (DEV / 'pid').write_text(str(p.pid))
    print(f'Started PID {p.pid}; isolated library: {DEV / "library"}')


def test(script):
    if not BIN.is_file():
        raise ValueError('No verified development app.')
    reports = DEV / 'reports'
    reports.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='reader-fixture-', dir=DEV) as lib:
        fixture = Path(lib)
        (fixture / 'Reading journey.md').write_text('# Reading journey\n\n' + '\n\n'.join(
            f'## Chapter {i}\n\nParagraph {i}. A quiet place to read, bookmark and return. ' + 'Reading stays local. ' * 25 for i in range(1, 31)))
        (fixture / '.quiet').mkdir()
        (fixture / '.quiet/preferences.json').write_text(json.dumps(dict(language='en', english=True, auto_update=False, usage_stats=False, quick_hotkey=False)))
        (fixture / 'Connected notes.md').write_text('# Connected notes\n\n[Reading journey](Reading%20journey.md)\n')
        work = reports / time.strftime('%Y%m%d-%H%M%S')
        run('python3', 'tools/harness/run.py', str(Path(script).resolve()), str(fixture), str(work),
            env={**os.environ, 'FDL_BIN': str(BIN), 'FDL_NOBLINK': '1', 'FDL_RUN_TIMEOUT': '180'})
        before = json.loads((work / 'reader-state.json').read_text())
        restart = work / 'restart.txt'
        restart.write_text('resize 1280 860\nwait 1000\nrow Reading journey\nwait 800\nreader-expect /read_only true\nreader-expect /shelved true\nreader-expect /bookmarks 1\nanchor reader-bookmark-add\nwait 300\nreader-expect /bookmarks 1\nreader-state reader-restored.json\nshot reader-restored\nquit\n')
        env = {**os.environ, 'QUIET_MD_HOME': str(work / 'lib'), 'QUIET_MD_LOG_DIR': str(work / 'logs'),
            'FDL_SCRIPT': str(restart), 'FDL_SHOTS': str(work), 'FDL_NOBLINK': '1', 'DO_NOT_TRACK': '1'}
        with (work / 'restart.log').open('w') as f:
            run(str(BIN), env=env, stdout=f, stderr=subprocess.STDOUT, timeout=180)
        after = json.loads((work / 'reader-restored.json').read_text())
        if before['position']['quote'] != after['position']['quote'] or after['furthest'] <= 0:
            raise ValueError('Reading position was not restored after restart.')
        if (work / 'lib/Reading journey.md').read_bytes() != (fixture / 'Reading journey.md').read_bytes():
            raise ValueError('Reading unexpectedly changed the document body.')
        print(f'Report: {work}; restart, bookmark deduplication and unchanged body verified.')
    # Reports contain only this generated fixture, never the user's library.
    for old in sorted(reports.iterdir(), key=lambda p: p.stat().st_mtime, reverse=True)[3:]:
        if old.is_dir():
            shutil.rmtree(old)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('command', choices=['doctor', 'build', 'check', 'status', 'watch', 'ci-logs', 'pull', 'run',
        'stop', 'restart', 'logs', 'test', 'inspect', 'clean', 'sync'])
    p.add_argument('value', nargs='?')
    a = p.parse_args()
    if a.command == 'doctor':
        print(json.dumps(dict(architecture=platform.machine(), free_gb=round(shutil.disk_usage(ROOT).free / 2**30, 1),
            tools={n: shutil.which(n) for n in ('git', 'gh', 'python3', 'ditto', 'codesign')}, local_compilation=False), indent=2))
        run('gh', 'auth', 'status')
    elif a.command in ('build', 'check', 'sync'):
        if run('git', 'status', '--porcelain', capture=True).strip():
            raise ValueError('Commit changes first; cloud builds cannot contain uncommitted source.')
        branch = run('git', 'branch', '--show-current', capture=True).strip()
        if not branch.startswith('codex/'):
            raise ValueError('Cloud development uses a codex/ branch.')
        run('gh', 'workflow', 'run', 'dev-build.yml', '--ref', branch, '-f', f'check_only={str(a.command == "check").lower()}')
        if a.command == 'sync':
            time.sleep(5)
            info = selected_run()
            run('gh', 'run', 'watch', str(info['databaseId']), '--exit-status', '--interval', '10')
            pull(info['databaseId'])
            launch()
    elif a.command in ('status', 'watch', 'ci-logs'):
        info = selected_run(a.value)
        if a.command == 'status':
            print(json.dumps(info, indent=2))
        elif a.command == 'watch':
            run('gh', 'run', 'watch', str(info['databaseId']), '--exit-status', '--interval', '10')
        else:
            run('gh', 'run', 'view', str(info['databaseId']), '--log-failed')
    elif a.command == 'pull':
        pull(a.value)
    elif a.command in ('run', 'restart'):
        launch()
    elif a.command == 'stop':
        stop()
    elif a.command == 'logs':
        run('tail', '-n', '100', str(DEV / 'logs/console.log'))
    elif a.command == 'test':
        test(a.value or ROOT / 'tools/harness/examples/reader.txt')
    elif a.command == 'inspect':
        states = sorted((DEV / 'reports').glob('*/reader-state.json'), key=lambda p: p.stat().st_mtime)
        if not states:
            raise ValueError('No state snapshot. Run the reader harness test first.')
        print(states[-1].read_text())
    elif a.command == 'clean':
        for name in ('replaced', 'previous'):
            if (DEV / name).is_dir():
                shutil.rmtree(DEV / name)
        print('Only the current test app is kept. Runtime library and reports were preserved.')


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as e:
        sys.exit(f'dev: {e}')
