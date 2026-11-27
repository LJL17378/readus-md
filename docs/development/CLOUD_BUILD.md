# Readus.md cloud development

The Mac edits source and runs the app. GitHub Actions compiles and runs Rust tests. Do not install a local toolchain or generate a local `target/` to work around a failed cloud build.

## Candidate and checks

After explicit authorization, commit a candidate on a `codex/` branch and push it to the user's fork. `.github/workflows/dev-build.yml` starts automatically. It uses the pinned Rust toolchain, `Cargo.lock`, one macOS architecture, a dependency cache and an isolated test library. It checks download-replacement safety and types/lints before expensive code generation, runs core regressions, builds with `devtools`, packages an ad-hoc signed app, then runs the repository's complete `scripts/check.sh` (tests, bundled-guide round trips and clippy, including devtools). A package is downloadable only after the entire run succeeds.

The artifact contains an ad-hoc signed app ZIP, a drag-to-install DMG, SHA-256 checksums and source metadata. It is not Apple-notarized. Building does not publish a release; publishing a test prerelease is a separate, explicitly dispatched workflow described below. `READUS_DEV_BUILD` changes the application identifier and disables upstream automatic updates; telemetry is disabled for devtools and isolated libraries already. Double-click launches default to `~/Library/Application Support/Readus.md Test/library`, with a separate sibling `logs` directory; explicit environment paths take precedence. The Readus icon is used by the app bundle.

```sh
scripts/dev.sh doctor
scripts/dev.sh status
scripts/dev.sh ci-logs
scripts/dev.sh pull <run-id>
scripts/dev.sh test
scripts/dev.sh inspect
```

`build` and `check` manually dispatch a development run on the current committed `codex/` branch. `sync` dispatches, waits, downloads and launches. The helper inherits the existing macOS system HTTP proxy for `gh` when no explicit proxy environment is set, without changing system settings. These commands require authenticated `gh` and a clean working tree. They do not automatically commit or push. Do not use a last successful build as evidence about uncommitted changes.

## Local artifacts and user data

Everything managed by the helper lives in ignored `.dev/`. `pull` checks the run's success, exact commit, repository, run ID, CPU architecture, devtools flag, SHA-256 and code signature before changing the installed test app. A failed validation leaves the current app intact. After successful replacement, the old app and downloaded archive are deleted: the user specifically requests only one test version. There is no previous-version cache.

`run` starts `.dev/current/Readus.md Dev.app` with `QUIET_MD_HOME=.dev/library` and a dedicated log directory. `stop` checks the recorded process's executable path before terminating it. Do not kill another Quiet.md process. `test` uses a generated throwaway library containing only two synthetic Markdown documents. The newest three report directories are kept. Run `python3 tools/dev/test_dev.py` to check replacement safety without network access or an app. Use `stop` after manual testing and verify that no test process remains.

## Runtime acceptance

The reader harness clicks the actual mode, shelf, bookmark and navigation controls by their tracked names, checks structured state, and captures the reader, bookshelf, global bookmarks and knowledge library. It exercises return navigation and bookmark jumps, then reuses the same synthetic library in a second app process to check position restoration, bookmark deduplication and byte-for-byte unchanged Markdown. Inspect the PNGs in light and dark, wide and narrow windows. The Progress component has a live example on the component gallery's card page.

`reader-state <filename.json>` writes a development-only state snapshot into `FDL_SHOTS`; `reader-expect /json/pointer <json-value>` fails the harness on an unexpected state. Neither interface exists in release builds without `devtools`. A restart test must reuse the first run's synthetic library directly; `tools/harness/run.py` deliberately recreates its working copy on every invocation, so two ordinary harness runs do not prove persistence.

Record the commit, Actions run, checks actually passed, local scenarios, screenshots inspected and remaining limitations in the task report. A candidate committed or compiled is not a verified feature.

The broader `main` CI also builds the separate `dist` profile and both Mac architectures. Its cold-build limits are 180 minutes on macOS and 90 minutes on Windows; the original 45-minute Windows limit canceled a first build during packaging after its tests and clippy had passed. The single-architecture development workflow remains the faster local-test delivery route. A timeout is not a compiler error, and increasing a limit does not establish that the pending run has passed.

## Publish a downloadable test package

After the exact development commit passes the full cloud check and local runtime acceptance, fast-forward it into `main`. With authorization to publish a test build, dispatch:

```sh
gh workflow run publish-test.yml --ref main -f development_run=<successful-run-id>
```

The workflow uses `tools/dev/publish.py` to require a successful **Readus development** run for the workflow's exact commit. It downloads the `readus-dev` artifact, checks provenance, architecture, both package checksums and the app signature, then creates a **prerelease** tagged `readus-test-<short-commit>`. It never silently replaces an existing release. Assets are the DMG, ZIP, `SHA256SUMS.txt` and `metadata.json`; no Rust compilation happens in this publishing job. The tag identifies a test snapshot of the upstream-base version, not a new stable semantic version.

User downloads belong in [this repository's Releases](https://github.com/LJL17378/readus-md/releases). GitHub's `/releases/latest` excludes prereleases. GitHub Packages is not configured and is not the desktop installer entry. Actions Artifacts expire after seven days; release assets are persistent. Read the installation and signing limits in [the test release notes](../releases/readus-test.md). Do not run upstream CDN publication scripts for Readus.md.
