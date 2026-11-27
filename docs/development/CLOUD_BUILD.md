# Readus.md cloud development

The Mac edits source and runs the app. GitHub Actions compiles and runs Rust tests. Do not install a local toolchain or generate a local `target/` to work around a failed cloud build.

## Candidate and checks

After explicit authorization, commit a candidate on a `codex/` branch and push it to the user's fork. `.github/workflows/dev-build.yml` starts automatically. It uses the pinned Rust toolchain, `Cargo.lock`, one macOS architecture, a dependency cache and an isolated test library. It runs core regressions, builds with `devtools`, packages an ad-hoc signed app, then runs the repository's complete `scripts/check.sh` (tests, bundled-guide round trips and clippy, including devtools). A package is downloadable only after the entire run succeeds.

There is no DMG, notarization or public release in this workflow. `READUS_DEV_BUILD` changes the application identifier and disables upstream automatic updates; telemetry is disabled for devtools and isolated libraries already. The new Readus icon is used by the app bundle.

```sh
scripts/dev.sh doctor
scripts/dev.sh status
scripts/dev.sh ci-logs
scripts/dev.sh pull <run-id>
scripts/dev.sh test
scripts/dev.sh inspect
```

`build` and `check` manually dispatch a development run on the current committed `codex/` branch. `sync` dispatches, waits, downloads and launches. These commands require authenticated `gh` and a clean working tree. They do not automatically commit or push. Do not use a last successful build as evidence about uncommitted changes.

## Local artifacts and user data

Everything managed by the helper lives in ignored `.dev/`. `pull` checks the run's success, exact commit, repository, run ID, CPU architecture, devtools flag, SHA-256 and code signature before changing the installed test app. A failed validation leaves the current app intact. After successful replacement, the old app and downloaded archive are deleted: the user specifically requests only one test version. There is no previous-version cache.

`run` starts `.dev/current/Readus.md Dev.app` with `QUIET_MD_HOME=.dev/library` and a dedicated log directory. `stop` checks the recorded process's executable path before terminating it. Do not kill another Quiet.md process. `test` uses a generated throwaway library containing only two synthetic Markdown documents. The newest three report directories are kept. Run `python3 tools/dev/test_dev.py` to check replacement safety without network access or an app. Use `stop` after manual testing and verify that no test process remains.

## Runtime acceptance

The reader harness clicks the actual mode, shelf, bookmark and navigation controls by their tracked names, checks structured state, and captures the reader, bookshelf, global bookmarks and knowledge library. It exercises return navigation and bookmark jumps, then reuses the same synthetic library in a second app process to check position restoration, bookmark deduplication and byte-for-byte unchanged Markdown. Inspect the PNGs in light and dark, wide and narrow windows. The Progress component has a live example on the component gallery's card page.

`reader-state <filename.json>` writes a development-only state snapshot into `FDL_SHOTS`; `reader-expect /json/pointer <json-value>` fails the harness on an unexpected state. Neither interface exists in release builds without `devtools`. A restart test must reuse the first run's synthetic library directly; `tools/harness/run.py` deliberately recreates its working copy on every invocation, so two ordinary harness runs do not prove persistence.

Record the commit, Actions run, checks actually passed, local scenarios, screenshots inspected and remaining limitations in the task report. A candidate committed or compiled is not a verified feature.
