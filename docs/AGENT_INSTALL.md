# Agent integration in Readus.md

Readus.md retains the upstream `quiet` CLI source, Session API and external agent integration. **The current Readus.md Test package contains the desktop app only; it does not bundle `quiet`.** Installing the upstream Quiet.md app or its skill is not a way to obtain Readus.md's bookshelf and reading features.

For the current desktop test build, use [Readus.md Releases](https://github.com/LJL17378/readus-md/releases) and [the reading guide](READING.md). For development and automated runtime acceptance, use [the cloud workflow](development/CLOUD_BUILD.md).

Contributors who already have a separately built `quiet` CLI can consult [the inherited CLI API documentation](design/CLI.md). The `skills/quiet-md` directory and its upstream installation commands are retained upstream integration material; they are not a claim that the current Readus test installer supplies the CLI or an embedded Agent.

No built-in Agent chat has been implemented. A future implementation must include provider, endpoint, model, API-key creation/replacement/deletion and a connection test. Store secrets in the operating system's credential store, mask them in the UI and keep them out of documents, Git, logs and public build artifacts. See [the acceptance requirements](READING.md#后续-agent-功能的共同验收要求).
