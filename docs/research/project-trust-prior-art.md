# Study: prior art for project configuration trust

Date: 2026-10-10. Input for [project-config.md](../design/project-config.md).
This report replaces the summary in [project-trust.md](project-trust.md)
with cited facts.

**Method.** Three research agents read the source code and the official
documentation of each tool on 2026-10-10, at the versions in each part.
Each fact has a citation and a mark: V (the agent read the source or the
document), I (inferred, often from a search that found nothing) or U (not
verifiable). The facts have no recommendations. The cloned sources were
read only.

Parts:
1. MCP clients with a command line: Claude Code, Codex, Gemini CLI.
2. Editors: VS Code, Cursor, Zed, Goose, OpenCode.
3. Developer tools with a trust step: direnv, mise, git, VS Code
   Workspace Trust, Nix, Cargo.


---

## Part: Prior art: project MCP configuration trust in CLI agents

Date of research: 2026-10-10. Facts only. No recommendations.

### Sources and versions

| ID | Source | Version / commit |
| --- | --- | --- |
| CX | github.com/openai/codex (Rust source, `codex-rs/`) | `main` at `806d9732c974bc8a51b8317c1bd8985544fe627c` (2026-10-10) |
| CXD | Codex docs: learn.chatgpt.com/docs/config-file/config-advanced, learn.chatgpt.com/docs/extend/mcp?surface=cli (redirect targets of developers.openai.com/codex/config-advanced and /codex/mcp) | fetched 2026-10-10 |
| GM | github.com/google-gemini/gemini-cli | `main` at `9b6e0265d16bbd29ca51e33c9e0c01dc4cec5e83`, package version `0.65.0-nightly.20261006` |
| GMD | Gemini CLI docs in the same repo: `docs/cli/trusted-folders.md`, `docs/tools/mcp-server.md` | same commit |
| CC | Claude Code docs: code.claude.com/docs/en/{mcp,settings,settings-reference,permissions,security,headless,claude-directory}.md | fetched 2026-10-10 |
| CCL | github.com/anthropics/claude-code `CHANGELOG.md` | `2301018b1f61073c501a8e7a4813ef48c239163b` (latest entry 2.1.296) |

Claude Code is closed source. The GitHub repository has the changelog, examples and plugins only. All Claude Code facts come from official docs, the changelog and GitHub Security Advisories. Nothing below about Claude Code internals is from source code.

Labels: **V-src** = verified, I read the source. **V-doc** = verified, I read the official doc or advisory. **Inferred** = not stated directly. **Unverified** = I could not confirm it.

---

### Q1. Which project MCP files, and how the tool finds them

| Tool | Fact | Citation | Status |
| --- | --- | --- | --- |
| Codex | Project MCP servers go in `[mcp_servers.*]` in `.codex/config.toml`. There is no `.mcp.json` support. Docs: "`.codex/config.toml` (trusted projects only)". | CXD mcp page; CX `codex-rs/config/src/mcp_types.rs` (`RawMcpServerConfig`) | V-doc, V-src |
| Codex | Loader comment: "tree: parent directories up to root looking for `./.codex/config.toml` (loaded but disabled when untrusted)"; "repo: `$(git rev-parse --show-toplevel)/.codex/config.toml`". | CX `codex-rs/config/src/loader/mod.rs` lines ~132-134 | V-src |
| Codex | It walks from cwd up to the project root and loads every `.codex/config.toml` in that range. The closest file wins: `discover_project_layers` collects `cwd.ancestors()` until `project_root`, then reverses the list. | CX `loader/mod.rs` `discover_project_layers`; CXD: "Codex walks from the project root to your current working directory and loads every `.codex/config.toml` it finds." | V-src, V-doc |
| Codex | Project root = the nearest ancestor that has a marker from `project_root_markers` (default `[".git"]`). A `.git` directory without `HEAD` is skipped. An empty list means "cwd is the root". | CX `config/src/project_root_markers.rs` (`DEFAULT_PROJECT_ROOT_MARKERS: &[".git"]`); `loader/mod.rs` `discover_project_root` | V-src |
| Codex | Markers are read only from non-project layers (system, user, CLI, managed), so a repository cannot change its own root. Comment: "Callers must use markers from configuration loaded before project layers." | CX `loader/mod.rs` `find_project_root` doc comment; `load_config_layers_state` builds `merged_so_far` from non-project layers | V-src |
| Codex | A `.codex` folder that equals `CODEX_HOME` (also after canonicalization) is skipped as a project layer. | CX `loader/mod.rs` `discover_project_layers` (`dot_codex_abs == codex_home_abs`) | V-src |
| Gemini | Project file is `<cwd>/.gemini/settings.json`, key `mcpServers`. No upward search: `loadSettings(workspaceDir = process.cwd())`, `getWorkspaceSettingsPath()` = `path.join(targetDir, ".gemini", "settings.json")`. | GM `packages/cli/src/config/settings.ts` `loadSettings`; `packages/core/src/config/storage.ts` `getGeminiDir`, `getWorkspaceSettingsPath` | V-src |
| Gemini | It does not read the workspace file when cwd is the home folder (real-path compare): `if (!storage.isWorkspaceHomeDir()) workspaceResult = load(...)`. | GM `settings.ts` `_doLoadSettings`; `storage.ts` `isWorkspaceHomeDir` | V-src |
| Gemini | Gemini CLI does not read `.mcp.json`. A search for the string `.mcp.json` in non-test `.ts` files returns no result. | GM grep at commit | V-src (absence) |
| Claude Code | Project MCP file is `.mcp.json`: "storing configurations in a `.mcp.json` file at your project's root directory". It "Lives at the project root, not inside `.claude/`". | CC mcp.md "Project scope"; claude-directory.md | V-doc |
| Claude Code | Docs do not say whether Claude Code searches parent folders for `.mcp.json`, or stops at the git root. Shared `.claude/settings.json` is read "from the session's primary working directory". | CC settings.md | Unverified for `.mcp.json` |
| Claude Code | Other MCP scopes: local and user scope are in `~/.claude.json` (local is under `projects["<path>"].mcpServers`). | CC mcp.md "MCP installation scopes" | V-doc |

### Q2. Unit of approval, and what the prompt shows

| Tool | Fact | Citation | Status |
| --- | --- | --- | --- |
| Codex | The unit is the folder (project). There is no approval for each MCP server. An untrusted project layer is loaded, but it is disabled as a whole: "To load project-local config, hooks, and exec policies, add {trust_key} as a trusted project in {user_config_file}." | CX `loader/mod.rs` `disabled_reason_for_decision`, `project_layer_entry` (`ConfigLayerEntry::new_disabled`) | V-src |
| Codex | The trust prompt shows the folder path and generic text only. It does not list servers, commands or env values. Snapshot text: "Trust this folder? Codex can read, edit, and run files here, subject to your permission settings. Folder settings can run code automatically, even without a model request. ... Your trust decision will be saved." Choices: "Trust and continue", "Quit". | CX `codex-rs/tui/src/onboarding/snapshots/codex_tui__onboarding__trust_directory__tests__renders_snapshot_for_git_repo.snap` | V-src |
| Codex | Some keys are always removed from project config, trusted or not (`PROJECT_LOCAL_CONFIG_DENYLIST`: `openai_base_url`, `chatgpt_base_url`, `model_provider`, `model_providers`, `notify`, `profile`, `profiles`, `otel`, ...). `mcp_servers` is not on the list. | CX `loader/mod.rs` lines ~89-102, `sanitize_project_config` | V-src |
| Codex | A project layer cannot add, replace or re-enable an enterprise (EMA auth) MCP registration: "project configuration cannot re-enable enterprise MCP server `{name}`". | CX `config/src/mcp_ema.rs` `validate_ema_auth_sources` | V-src |
| Gemini | The unit is the folder, with an option to trust the parent folder. Choices: "Trust folder (<dir>)", "Trust parent folder (<parent>)", "Don't trust". Title: "Do you trust the files in this folder?" | GM `packages/cli/src/ui/components/FolderTrustDialog.tsx` | V-src |
| Gemini | The dialog lists MCP servers by name only: `results.mcps = Object.keys(mcpServers)`. It also lists commands, hooks, skills, setting overrides and "Security Warnings" (for example auto-approve or no sandbox). It does not show command lines or env values. | GM `packages/core/src/services/FolderTrustDiscoveryService.ts`; GMD trusted-folders.md "discovery phase" | V-src, V-doc |
| Gemini | When the folder is untrusted, the workspace settings are dropped as a whole (`const safeWorkspace = isTrusted ? workspace : {}`), and no MCP server connects, user-level servers included (`if (!this.cliConfig.isTrustedFolder()) return;`). | GM `settings.ts` `mergeSettings`; `packages/core/src/tools/mcp-client-manager.ts` ~line 461; GMD: "MCP servers do not connect" | V-src, V-doc |
| Claude Code | Two layers. (1) A workspace trust dialog for the folder. (2) A separate approval for each `.mcp.json` server. Changelog 2.1.69: "Fixed trust dialog silently enabling all `.mcp.json` servers on first run. You'll now see the per-server approval dialog as expected". | CCL 2.1.69; CC security.md: "Servers in a project's `.mcp.json` have their own approval prompt" | V-doc |
| Claude Code | Per-server choices are saved by server name: approve → `enabledMcpjsonServers`; approve all → `enableAllProjectMcpServers: true`; reject → `disabledMcpjsonServers`. All three are written to `.claude/settings.local.json`. | CC settings-reference.md (entries for the three keys) | V-doc |
| Claude Code | The repository cannot approve its own servers. Approvals in a committed `.claude/settings.json` are ignored in an untrusted folder. Approvals from user settings, managed settings and `--settings` still apply. `disabledMcpjsonServers` in any file always rejects. | CC mcp.md "Project server approvals and workspace trust"; CCL 2.1.196 | V-doc |
| Claude Code | The content of the per-server approval dialog (full command? env?) is not documented. The trust dialog lists repository `permissions.allow` rules and additional directories, and names the repository root (CCL 2.1.218). | CC permissions.md; CCL 2.1.218 | Unverified for the MCP dialog |

### Q3. What triggers re-approval; hashing

| Tool | Fact | Citation | Status |
| --- | --- | --- | --- |
| Codex | Project trust is a path → `trust_level` map. No hash of `.codex/config.toml`. A change to the file does not cause a new prompt. | CX `config/src/config_toml.rs` `ProjectConfig.trust_level`; `loader/mod.rs` `ProjectTrustContext` | V-src (no hash in trust path) |
| Codex (adjacent, hooks only) | Hooks are trusted per entry by hash. `hook_hash` serializes the normalized hook identity (event, matcher, handler) and calls `version_for_toml`: SHA-256 of JSON with sorted object keys, formatted `sha256:<hex>`. A stored `trusted_hash` that differs gives `HookTrustStatus::Modified`. This is not applied to MCP servers. | CX `codex-rs/hooks/src/engine/discovery.rs` `hook_hash`, `hook_trust_status`; `config/src/fingerprint.rs` `version_for_toml` | V-src |
| Gemini | Trust is a path → `TrustLevel` map. No hash. A file change does not cause a new prompt. | GM `packages/core/src/utils/trust.ts` (`Record<string, TrustLevel>`) | V-src |
| Claude Code | Approvals are stored as server names (`enabledMcpjsonServers: ["memory","github"]`). Docs describe no content hash. A new server name has no approval and so needs one. | CC settings-reference.md | V-doc (name key) |
| Claude Code | A change to the command or URL of an already-approved name does not cause a new prompt. | Follows from the name-only key | Inferred, not verified |
| Claude Code | `claude mcp reset-project-choices` clears the saved choices. | CC mcp.md "Project scope" | V-doc |
| Claude Code (adjacent) | The managed-settings approval dialog lists "only the settings that changed since you last approved them" (CCL 2.1.251). That is a change-detection approval for managed settings, not for `.mcp.json`. | CCL 2.1.251 | V-doc |

### Q4. Where the trust record is stored, and its key

| Tool | Fact | Citation | Status |
| --- | --- | --- | --- |
| Codex | `$CODEX_HOME/config.toml` (default `~/.codex/config.toml`), as `[projects."<path>"] trust_level = "trusted"` or `"untrusted"`. | CX `codex-rs/core/src/config/mod.rs` `set_project_trust_level_inner` | V-src |
| Codex | The key that is written is `project_trust_key(path)`: `dunce::canonicalize` (real path, symlinks resolved), lowercased on Windows only. | CX `loader/mod.rs` `project_trust_key`, `normalized_project_trust_keys` (`use dunce::canonicalize as normalize_path`) | V-src |
| Codex | Lookup order: the dir of each `.codex` layer, then the project root (canonical, then original spelling), then the git repo root. The first match wins, so an explicit `untrusted` on a subfolder wins over a trusted root. | CX `loader/mod.rs` `ProjectTrustContext::decision_for_dir` | V-src |
| Codex | The repo root for trust is the main repository, not the worktree. Linked worktrees are resolved without running `git`: a `.git` file that is a symlink or larger than 64 KiB is rejected; `gitdir`/`commondir` must agree with the canonical checkout path. | CX `codex-rs/git-utils/src/trust.rs` `resolve_root` (`MAX_GIT_METADATA_FILE_BYTES`) | V-src |
| Codex | No inode key. No git commit or remote URL in the key. | CX same files | V-src (absence) |
| Gemini | `~/.gemini/trustedFolders.json` (path from `Storage.getTrustedFoldersPath()`), or the file in env var `GEMINI_CLI_TRUSTED_FOLDERS_PATH`. Under macOS `sandbox-exec` it is under `~/.cache/.gemini/`. Written with mode `0o600` through a temp file and rename, with a lock file. | GM `core/src/config/storage.ts`; `core/src/utils/trust.ts` `saveTrustedFolders`, `setValue` | V-src |
| Gemini | The key written is `normalizePath(cwd)`: absolute path, lowercased on macOS and Windows. It is not a real path at write time. When it matches, both the rule path and the location are passed through `fs.realpathSync`, and the longest matching rule wins. A rule covers all subfolders (`isSubpath`). `TRUST_PARENT` covers `dirname(rulePath)`. | GM `core/src/utils/trust.ts` `isPathTrusted`; `core/src/utils/paths.ts` `normalizePath` | V-src |
| Gemini | Git root is not used. The key is the cwd path. | GM `useFolderTrust.ts` (`trustedFolders.setValue(cwd, ...)`) | V-src |
| Claude Code | `~/.claude.json`, `projects["<path>"].hasTrustDialogAccepted`. | CC permissions.md "What runs before you trust a folder"; claude-directory.md | V-doc |
| Claude Code | Key: in a repository, the git repository root ("the trust covers the whole repository apart from any git repository nested inside it"). In a worktree, the main checkout's root. Outside a repository, the start folder, and the trust covers its subfolders. In the home folder, trust is kept for the session only and not written to disk. | CC permissions.md "Project allow rules and workspace trust"; security.md | V-doc |
| Claude Code | Whether the key is a real (symlink-resolved) path is not documented. | — | Unverified |
| Claude Code | Per-server MCP approvals are in `.claude/settings.local.json` (see Q2). That file is read and written at the repository root. It is not used, and the shared file is used instead, "when the repository root or its `.git` or `.claude` entry isn't owned by your user" or the root is the home folder. | CC settings.md lines on `.claude/settings.local.json` | V-doc |

### Q5. No TTY / headless; override flags and env vars

| Tool | Fact | Citation | Status |
| --- | --- | --- | --- |
| Codex | `codex exec` shows no trust prompt. An untrusted project layer stays disabled, so its `mcp_servers` do not load. The trust check is in the shared loader, not in the TUI. | CX `loader/mod.rs` (no TTY branch); `exec/src/lib.rs` | V-src |
| Codex | `codex exec` exits outside a git repo: "Not inside a trusted directory and --skip-git-repo-check was not specified." Flags `--skip-git-repo-check` and `--dangerously-bypass-approvals-and-sandbox` skip this check. This check is about git, not about project config trust. | CX `codex-rs/exec/src/lib.rs` ~line 1012; `exec/src/cli.rs` | V-src |
| Codex | CLI `-c` overrides are merged into the config that decides trust (`merged_so_far` includes `cli_overrides_layer`). So `-c 'projects."<path>".trust_level="trusted"'` should trust for one run without saving. | CX `loader/mod.rs` `load_config_layers_state` | Merge V-src; flag use inferred, not tested |
| Codex | Other related flags: `--ignore-user-config`, `--ignore-rules`, `--dangerously-bypass-hook-trust`. I found no env var that trusts a project. | CX `exec/src/cli.rs`, `cli/src/main.rs` | V-src (flags exist) |
| Gemini | Headless = `CI=true` or `GITHUB_ACTIONS=true`, or stdin/stdout not a TTY, or a prompt/query, or `-p`/`--prompt` in argv. | GM `core/src/utils/headless.ts` `isHeadlessMode` | V-src |
| Gemini | Headless and not trusted → `FatalUntrustedWorkspaceError`: "Gemini CLI is not running in a trusted directory. To proceed, either use `--skip-trust`, set the `GEMINI_CLI_TRUST_WORKSPACE=true` environment variable, or trust this directory in interactive mode." | GM `packages/cli/src/utils/userStartupWarnings.ts` `folderTrustCheck` | V-src |
| Gemini | `--skip-trust` sets `process.env.GEMINI_CLI_TRUST_WORKSPACE = "true"`. Not saved: "Trust the current workspace for this session." | GM `packages/cli/src/config/config.ts` ~lines 308, 516 | V-src |
| Gemini | Env vars: `GEMINI_CLI_TRUST_WORKSPACE=true` → trusted; `GEMINI_CLI_TRUST_WORKSPACE=false` or `GEMINI_RESTRICTED_MODE=true` → untrusted. Env wins over the IDE signal and the file. | GM `core/src/utils/trust.ts` `checkPathTrust` | V-src |
| Gemini | Folder trust is on by default in code: `settings.security?.folderTrust?.enabled ?? true`, schema `default: true`. The doc `docs/cli/trusted-folders.md` still says "disabled by default". The doc and the code do not agree. | GM `packages/cli/src/config/trustedFolders.ts`; `settingsSchema.ts`; GMD | V-src, V-doc (conflict) |
| Claude Code | `claude -p`, Agent SDK and cloud sessions show neither dialog: "it loads project-scoped servers without asking". `.mcp.json` servers are "Connected without asking, approved or not". | CC mcp.md "Project scope"; permissions.md table; headless.md | V-doc |
| Claude Code | No prompt also in `bypassPermissions` mode when `skipDangerousModePermissionPrompt` is set in user or managed settings. | CC mcp.md | V-doc |
| Claude Code | Controls for `-p`: `disabledMcpjsonServers`; `--setting-sources user` (or SDK `settingSources`); `--strict-mcp-config` (use only `--mcp-config`); `--bare` (reads no `.mcp.json`). | CC mcp.md; permissions.md; headless.md | V-doc |
| Claude Code | In `-p`, `headersHelper` in a project `.mcp.json` does not run unless the folder trust dialog was accepted earlier (since 2.1.238). | CCL 2.1.238; CC mcp.md | V-doc |
| Claude Code | `claude --bg` in an untrusted folder exits when it cannot ask (2.1.281). `claude mcp list` with piped output shows unapproved servers as `⏸ Pending approval` (2.1.154). | CCL | V-doc |
| Claude Code | Manual trust: set `projects["<path>"].hasTrustDialogAccepted` to `true` in `~/.claude.json`. A `-p` session does not save trust. | CC permissions.md | V-doc |

### Q6. Same name in project and user configuration

| Tool | Fact | Citation | Status |
| --- | --- | --- | --- |
| Codex | Project layers are above user layers. `merge_toml_values` merges tables recursively, so `[mcp_servers.X]` in the project overlays the user's `[mcp_servers.X]` key by key. Fields that the project does not set are kept from the user entry. | CX `config/src/loader/README.md` "Layering model"; `config/src/merge.rs` `merge_toml_values_at_path` | V-src |
| Codex | Exception: enterprise EMA-auth servers. A project cannot replace or re-enable them. | CX `config/src/mcp_ema.rs` | V-src |
| Gemini | `mcpServers` uses `MergeStrategy.SHALLOW_MERGE`: `{ ...user, ...workspace }`. The workspace entry replaces the user entry with the same name as a whole. Order: defaults < system defaults < user < workspace < system. | GM `packages/cli/src/config/settingsSchema.ts` (mcpServers); `packages/cli/src/utils/deepMerge.ts`; `settings.ts` `mergeSettings` comment | V-src |
| Claude Code | Local > Project > User > plugin > claude.ai connectors. "The entire server entry from that source is used; fields are not merged across scopes." Duplicates across the three scopes are matched by name. `managedMcpServers` ranks above all. | CC mcp.md "Scope hierarchy and precedence" | V-doc |
| Claude Code | So user scope does not override a project server with the same name. Project wins over user, and local wins over project. | CC mcp.md | V-doc |

### Q7. `${VAR}` expansion in project files

| Tool | Fact | Citation | Status |
| --- | --- | --- | --- |
| Codex | No `${VAR}` interpolation in MCP config. Env is passed by name: `env` (literal map), `env_vars` (names to forward, `source = "remote"` possible), `bearer_token_env_var`, `env_http_headers` (header → env var name). An inline `bearer_token` is rejected. | CX `config/src/mcp_types.rs` `RawMcpServerConfig`, `throw_if_set("streamable_http", "bearer_token", ...)`; CXD mcp page | V-src, V-doc |
| Codex | No project/user difference for these fields. | CX same | V-src (absence) |
| Codex (adjacent) | `.env` from `$CODEX_HOME/.env` only; keys that start with `CODEX_` are dropped ("Do not allow `.env` files to create or modify any variables with names starting with `CODEX_`"). | CX `codex-rs/arg0/src/lib.rs` `load_dotenv`, `set_filtered` | V-src |
| Gemini | `resolveEnvVarsInObject` runs on every string in every settings file (system, user, workspace) before validation. Syntax: `$VAR`, `${VAR}`, `${VAR:-default}`. An unset variable without a default stays as the literal text. | GM `packages/cli/src/utils/envVarResolver.ts`; `settings.ts` `load()` | V-src |
| Gemini | The doc says expansion is in the `env` block, supports `%VAR%` on Windows, and "If a variable is not defined in the current environment, it resolves to an empty string." This does not agree with the settings-load code above. | GMD `docs/tools/mcp-server.md` "Environment variable expansion" | V-doc (conflict with source) |
| Gemini | No project/user difference in expansion. There is an indirect limit: an untrusted workspace's settings are not used and its `.env` is not loaded. | GM `settings.ts`; GMD trusted-folders.md | V-src |
| Gemini (adjacent) | Stdio MCP servers get a sanitized base environment: names like `*TOKEN*`, `*SECRET*`, `*KEY*`, `*AUTH*` are removed unless the server's `env` names them explicitly. | GMD `docs/tools/mcp-server.md` "Security and environment sanitization" | V-doc |
| Claude Code | `.mcp.json` supports `${VAR}` and `${VAR:-default}` in `command`, `args`, `env`, `url`, `headers`. An unset variable without a default stays as literal `${VAR}`, with a warning in `claude mcp list`. | CC mcp.md "Environment variable expansion in `.mcp.json`" | V-doc |
| Claude Code | In a remote server's `url` and `headers`, credential variables read as empty, "whether or not you have set the variable", and `:-default` is ignored. Covered: Claude Code's own credentials (`ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`), cloud provider credentials (`AWS_BEARER_TOKEN_BEDROCK`), and others (`HTTPS_PROXY`, `NPM_TOKEN`). Reason: "This keeps a project's `.mcp.json` or a plugin from sending your Claude Code or cloud provider credentials to a server it names." | CC mcp.md "Credential variables that read as empty" | V-doc |
| Claude Code | The doc does not limit that rule to project scope by name. | CC mcp.md | V-doc (scope not stated) |
| Claude Code | A project or plugin `headersHelper` runs without inherited credential env vars (2.1.238). | CCL 2.1.238 | V-doc |

### Q8. Owner, permission and symlink checks on the project file

| Tool | Fact | Citation | Status |
| --- | --- | --- | --- |
| Codex | No owner or mode check on `.codex/config.toml`. It is read with `read_file_text`. | CX `loader/mod.rs` `discover_project_layers` | V-src (absence) |
| Codex | Symlinks: the trust key is canonicalized. In git worktree resolution, a `.git` file that is a symlink, or a gitdir that is a symlink, is rejected ("Following a substituted .git symlink must never turn an unrelated checkout into the registered one"). | CX `git-utils/src/trust.rs` | V-src |
| Gemini | No owner, mode or symlink check on `.gemini/settings.json`. It is read with `fs.readFileSync`. | GM `settings.ts` `load()` | V-src (absence) |
| Gemini | Real path is used for trust matching and for the home-folder check. The trust file is written with mode `0o600`. | GM `core/src/utils/trust.ts`; `storage.ts` `isWorkspaceHomeDir` | V-src |
| Claude Code | `.claude/settings.local.json` is treated as repository-supplied (held until trust) "When the file is tracked in git, or `.claude` is a symlink". | CC permissions.md "When your local settings file needs trust" | V-doc |
| Claude Code | Local settings move to the shared file "when the repository root or its `.git` or `.claude` entry isn't owned by your user". | CC settings.md | V-doc |
| Claude Code | `claude mcp add/remove` fails fast when `.mcp.json` is a FIFO or a device-file symlink (2.1.257). A `.claude` directory symlinked from outside the project asks for the external-imports approval (2.1.284). | CCL | V-doc |
| Claude Code | An owner or permission check on `.mcp.json` itself is not documented. | — | Unverified |

### Q9. Advisories and CVEs

| Tool | ID | What happened | Fix | Citation | Status |
| --- | --- | --- | --- | --- | --- |
| Codex | CVE-2025-61260 / GHSA-xrxf-jgv3-qmrm (critical) | A repo `.env` set `CODEX_HOME=./.codex`. Codex then read `./.codex/config.toml` as user config, and its `mcp_servers` command ran at startup: "no interactive approval, no secondary validation of the command or arguments". | Check Point: "The patch prevents .env files from silently redirecting CODEX_HOME into project directories", in 0.23.0 (2025-08-20). The current source loads `.env` only from `$CODEX_HOME/.env` and drops `CODEX_*` keys. | research.checkpoint.com/2025/openai-codex-cli-command-injection-vulnerability/; GitHub advisory GHSA-xrxf-jgv3-qmrm; CX `arg0/src/lib.rs` | V-doc, V-src |
| Codex | same | Version conflict: the GitHub advisory says "affected: <= 0.23.0, patched: null". Check Point says 0.23.0 is the fixed version. | — | same | V-doc (sources disagree) |
| Codex | CVE-2025-59532 / GHSA-w5fx-fh39-j5rw | Sandbox path configuration bug. Not about project config. | — | gh api openai/codex advisories | V-doc |
| Gemini | CVE-2026-12537 / GHSA-wpqr-6v78-jr5g (critical) | "Gemini CLI running in CI environments (headless mode) automatically trusted workspace folders for the purpose of loading configuration and environment variables", so a malicious `.gemini/` could lead to RCE. A second bug: `--yolo` ignored tool allowlists. | Affected < 0.39.1 (and 0.40.0-preview.2). Patched 0.39.1 / 0.40.0-preview.3; run-gemini-cli action 0.1.22. "aligns headless mode behavior with interactive mode, requiring folders to be explicitly trusted". The advisory text names `GEMINI_TRUST_WORKSPACE: 'true'` (workflow setting). The CLI source reads `GEMINI_CLI_TRUST_WORKSPACE`. | GitHub advisory GHSA-wpqr-6v78-jr5g; GM `trust.ts` | V-doc, V-src |
| Claude Code | CVE-2025-59536 / GHSA-4fgq-fpq9-mr3g (high) | "Claude Code could be tricked to execute code contained in a project before the user accepted the startup trust dialog." Check Point: `enableAllProjectMcpServers` / `enabledMcpjsonServers` in a repo `.claude/settings.json` auto-approved `.mcp.json` servers; "our command executed immediately upon running claude – before the user could even read the trust dialog." | Patched v1.0.111. Fix (Check Point): "MCP servers cannot execute before user approval". | GHSA-4fgq-fpq9-mr3g; research.checkpoint.com/2026/rce-and-api-token-exfiltration-through-claude-code-project-files-cve-2025-59536/ | V-doc |
| Claude Code | GHSA-ph6w-f82w-28w6 (no CVE) | Project hooks ran without clear warning. The trust warning "did not properly document that selecting 'Yes, proceed' would allow Claude Code to execute files in the folder". | Patched v1.0.87, with clearer dialog text. | GHSA-ph6w-f82w-28w6 | V-doc |
| Claude Code | CVE-2026-21852 / GHSA-jh7p-qr78-84p7 (medium) | A repo settings file set `ANTHROPIC_BASE_URL`, and API requests (with the API key) went out before the trust prompt. | Patched v2.0.65: "no API requests are initiated before users confirm the trust dialog" (Check Point). | GHSA-jh7p-qr78-84p7; Check Point article | V-doc |
| Claude Code | CVE-2026-33068 / GHSA-mmgp-wc2j-qcv7 (high) | A repo `.claude/settings.json` with `permissions.defaultMode: bypassPermissions` skipped the trust dialog. | Patched 2.1.53. The permission mode is no longer read from repo settings before the trust decision. | GHSA-mmgp-wc2j-qcv7 | V-doc |
| Claude Code | CVE-2026-40068 / GHSA-q5hj-mxqh-vv77 (high) | A crafted git worktree `commondir` file pointed at a path the victim had trusted earlier, so the trust dialog was skipped and repo hooks ran. | Patched 2.1.84 (affected >= 2.1.63). `commondir` is now validated. | GHSA-q5hj-mxqh-vv77 | V-doc |
| Claude Code | CVE-2025-65099 / GHSA-5hhx-v7f6-x7gv (high) | Yarn config in the repo ran code through `yarn --version` before trust. | Patched v1.0.39. | GHSA-5hhx-v7f6-x7gv | V-doc |
| Claude Code | Changelog security items (no GHSA) | 2.1.196: "`claude mcp list`/`get` no longer spawn `.mcp.json` servers that a repo self-approved via a committed `.claude/settings.json`". 2.1.207 (docs): untracked `settings.local.json` approvals no longer apply before trust. 2.1.238: project `headersHelper` needs folder trust, also under `-p`. | — | CCL; CC mcp.md | V-doc |

---

### Open points (not verified)

- Claude Code: whether `.mcp.json` is also read from parent folders, and where the search stops.
- Claude Code: what the per-server approval dialog shows (command, args, env values).
- Claude Code: whether a change to an approved server's content causes a new prompt. The docs show only a name-based key.
- Claude Code: whether the trust key is a real path.
- Codex: the `-c projects."<path>".trust_level=...` override is derived from the merge code. I did not run it.

---

## Part: Prior art: project MCP configuration in editors (facts only)

Date of research: 2026-10-10. No recommendations.

### Sources and versions

| Tool | Source | Version / commit |
|---|---|---|
| VS Code | github.com/microsoft/vscode, `main` | `959031245ebb1fe077e0d512e1397c0ae82006e4` (2026-10-10) |
| VS Code docs | code.visualstudio.com/docs/copilot/customization/mcp-servers; code.visualstudio.com/docs/agents/reference/mcp-configuration; code.visualstudio.com/docs/editing/workspaces/workspace-trust | read 2026-10-10 |
| Zed | github.com/zed-industries/zed, `main` | `f16f9652ec57bf806e65b2a0d51bb92a63644914` (2026-10-09), crate `zed` 1.25.0 |
| Goose | github.com/block/goose, `main` | `3bd852002903e016ff30947e973f76e2fcfcf90f` (2026-10-09), workspace 1.54.0 |
| OpenCode | github.com/sst/opencode, `dev` | `055d95bb7e278c94baf06235a52cac79dd13ba67` (2026-10-09), `opencode` 1.18.35 |
| Cursor | cursor.com/docs/context/mcp; cursor.com/docs/agent/security; cursor.com/docs/cli/mcp; cursor.com/docs/cli/reference/parameters; GHSA-24mc-g4xr-4395 (CVE-2025-54136); GHSA-4cxx-hrm3-49rm (CVE-2025-54135); research.checkpoint.com/2025/cursor-vulnerability-mcpoison/ | read 2026-10-10 |

Paths for VS Code below are relative to `src/vs/`. "V" = verified (read source or doc). "I" = inferred. "U" = could not verify.

---

### 1. Project-level files read, and how they are found

| Tool | Fact | Status | Citation |
|---|---|---|---|
| VS Code | Reads `<folder>/.vscode/mcp.json` for each workspace folder. Docs: "create or open `.vscode/mcp.json` in your project". | V (doc) | mcp-servers doc |
| VS Code | Reads `<folder>/.mcp.json` at each workspace folder root (added in PR #312234, merged 2026-04-23). Code: `joinPath(folder.uri, WORKSPACE_ROOT_MCP_CONFIG_FILE)` with `WORKSPACE_ROOT_MCP_CONFIG_FILE = '.mcp.json'`. No upward search. Accepts `{ "mcpServers": {...} }` or a bare server map. | V (source) | `workbench/contrib/mcp/common/discovery/workspaceDotMcpDiscovery.ts` L60; `platform/mcp/common/mcpWorkspaceConfiguration.ts` L18, L86-104 |
| VS Code | Also reads `settings.mcp` in a `.code-workspace` file (provenance `WorkspaceConfiguration`). | V (source) | `workbench/contrib/mcp/common/mcpTypes.ts` L55 |
| VS Code | Reads `<folder>/.cursor/mcp.json` only when discovery source `cursor-workspace` is on (default off). | V (source) | `workbench/contrib/mcp/common/discovery/workspaceMcpDiscoveryAdapter.ts` L52; `mcpConfiguration.ts` L48-54 |
| Zed | Reads `context_servers` from project settings `.zed/settings.json`. Server list for MCP uses the settings at each visible worktree root (`path: RelPath::empty()`); first worktree that defines a name wins (`or_insert_with`). | V (source) | `crates/project/src/context_server_store.rs` L1169-1191 |
| Zed | Does not read `.mcp.json`, `.vscode/mcp.json` or `.cursor/mcp.json` (no reference found). | I (search) | grep of `crates/` |
| Cursor | Project file `.cursor/mcp.json`; global `~/.cursor/mcp.json`. "Create `.cursor/mcp.json` in your project for project-specific tools." | V (doc) | cursor.com/docs/context/mcp |
| Cursor | CLI: "The CLI follows the same configuration precedence as the editor (project → global → nested)" and "automatically discovering configurations from parent directories". Exact search rules not documented. | V (doc), rules U | cursor.com/docs/cli/mcp |
| Goose | Does not read a repository-root `.mcp.json`. It reads `.mcp.json` inside each project plugin directory `<cwd>/.agents/plugins/<name>/` (`project_root.join(".agents").join("plugins")`; `DEFAULT_MCP_CONFIG = ".mcp.json"`). CLI uses `std::env::current_dir()` as project root. No upward search. | V (source) | `crates/goose/src/plugins/discovery.rs` L64-75, L181-183; `crates/goose/src/plugins/mcp_servers.rs` L12, L83-103; `crates/goose-cli/src/session/builder.rs` L591-597 |
| Goose | Docs: "Project plugin: `<project>/.agents/plugins/<plugin-name>/` — Available when goose is working in that project." | V (doc) | `documentation/docs/guides/context-engineering/plugins.md` L114 |
| OpenCode | Reads `mcp` key from `opencode.json` / `opencode.jsonc`, searched upward from the current directory to the worktree (git root): `afs.up({ targets: [name.jsonc, name.json], start: directory, stop: worktree })`, merged outermost first. Also `.opencode/opencode.json[c]` in each `.opencode` directory found upward. Does not read `.mcp.json`. | V (source) | `packages/opencode/src/config/paths.ts` L10-41; `config.ts` L420-446 |
| OpenCode | Docs: "it first looks for a config file in the current directory, then traverses up to the nearest Git directory." | V (doc) | `packages/web/src/content/docs/config.mdx` L119 |

### 2. Unit of approval, and what the user sees

| Tool | Fact | Status | Citation |
|---|---|---|---|
| VS Code | `.vscode/mcp.json`, `.mcp.json`, `.code-workspace` servers: unit is the whole workspace (Workspace Trust). Doc: "When you trust a workspace, servers in `.vscode/mcp.json` and workspace-root `.mcp.json` can start". Code: `if (collection.scope === StorageScope.WORKSPACE && !isWorkspaceTrusted()) ... requestWorkspaceTrust({ message: "This MCP server definition is defined in your workspace files." })`. These collections have `trustBehavior: Trusted` (no per-server prompt). | V (doc + source) | mcp-servers doc; `mcpRegistry.ts` L232-241; `installedMcpServersDiscovery.ts` L136; `workspaceDotMcpDiscovery.ts` L70 |
| VS Code | History: before PR #268959 (merged 2025-09-29, "mcp: don't trust-prompt workspace MCPs"), workspace-folder servers used `TrustedOnNonce` (per-server prompt). PR text: "The concerns for which this was initially added are now covered with the combination of workspace trust and sensitive file approvals." | V (PR) | github.com/microsoft/vscode/pull/268959, commit `2a3179b4df` |
| VS Code | Other-app sources (Cursor workspace/global, Claude Desktop, Windsurf): unit is each server entry (`TrustedOnNonce`). Workspace Trust is also required when the collection scope is workspace (`.cursor/mcp.json`). | V (source) | `workspaceMcpDiscoveryAdapter.ts` L58-59; `nativeMcpDiscoveryAbstract.ts` L149 |
| VS Code | Per-server dialog: title "Trust and run MCP server {0}?", body "The MCP server {0} was updated. MCP servers may add context to your chat session and lead to unexpected behavior. Do you want to trust and run this server?", buttons "Trust" / "Do not trust". The label links to the config location; the dialog does not show command or env. Several servers: "Trust" / "Pick Trusted" / "Do not trust". | V (source) | `mcpRegistry.ts` L326-382 |
| Zed | Unit is the worktree (directory), or a parent directory that covers it. Doc: "all worktrees start in Restricted Mode. Restricted Mode prevents downloading and running related items from `.zed/settings.json`." | V (doc) | `docs/src/worktree-trust.md` |
| Zed | Modal text: "Untrusted projects are opened in Restricted Mode to protect your system.", "Review .zed/settings.json for any extensions or commands configured by this project.", list "Project settings from being applied / Language servers from running / MCP Server integrations from installing"; buttons "Stay in Restricted Mode", "Trust and Continue". No server command or env is shown. | V (source) | `crates/workspace/src/security_modal.rs` L189-206, L276, L291 |
| Zed | Untrusted worktree: project settings are not parsed into the store; content is held in `pending_local_settings` and applied on the `Trusted` event. | V (source) | `crates/project/src/project_settings.rs` L1352-1384, L859-905 |
| Cursor | "All MCP connections need your approval." "After you approve an MCP connection, each tool call still needs individual approval before running." Unit per CVE-2025-54136 fix text: each `mcpServer` entry. Content of the prompt (command, env) is not documented. | V (doc); prompt content U | cursor.com/docs/agent/security; GHSA-24mc-g4xr-4395 |
| Goose | No approval prompt for project plugins. Newly found plugins are added to `config.yaml` with `enabled: true`: "Newly discovered plugins are added to the map with `enabled: true`". Doc warning: "Install plugins only from sources you trust, and review plugin contents before enabling them." | V (source + doc) | `crates/goose/src/plugins/discovery.rs` L120-151; `plugins.md` L13-14 |
| OpenCode | No trust or approval step for project config found. Project `mcp` servers with `enabled: true` start like global ones. | I (search for "trust" in `packages/opencode/src`, `packages/core/src` found none) | — |

### 3. What triggers re-approval; hash or nonce

| Tool | Fact | Status | Citation |
|---|---|---|---|
| VS Code (workspace files) | No re-approval on change. Workspace Trust is per folder URI, not per content. Changes to `.vscode/mcp.json` / `.mcp.json` in a trusted folder apply without a prompt. | V (source) for no per-server check; I for "no other prompt" | `mcpRegistry.ts` L240-242 |
| VS Code (other-app sources) | Per-server nonce. `cacheNonce = McpServerLaunch.hash(launch)`; `hash` = SHA-256 hex of `JSON.stringify(launch)`. `launch` holds stdio `type, command, args, env, envFile, cwd` (cwd = folder fsPath when not set) or HTTP `type, uri, headers`. Hash is of the raw entry, before variable resolution. Prompt when `definition.cacheNonce !== trustedAtNonce`. A denied server stores `'__vscode_not_trusted'` and is not prompted again (default `promptType: 'only-new'`). | V (source) | `mcpTypes.ts` L828-831; `nativeMcpDiscoveryAdapters.ts` L42-66; `mcpRegistry.ts` L43, L243-285 |
| VS Code | Doc: "VS Code shows a dialog when a server first starts or its configuration changes." | V (doc) | mcp-servers doc |
| Zed | No re-approval on change. Trust is stored by path; later edits of `.zed/settings.json` apply without a prompt. | I (trust check uses path only: `can_trust(worktree_store, worktree_id)`) | `project_settings.rs` L1361-1365 |
| Cursor | After fix: "Cursor now requires user approval every time an mcpServer entry is modified (in addition to the existing behavior of asking for approval when a new server is added)." Check Point: "any change to an MCP configuration—including something as small as adding a space—triggers a mandatory approval prompt." Hash method not published. | V (advisory, reporter blog); method U | GHSA-24mc-g4xr-4395; Check Point Research |
| Goose | None. Record key is the plugin root path; contents are not checked. | V (source) | `discovery.rs` L130-141 |
| OpenCode | None found. | I | — |

### 4. Where the trust record is stored, and its key

| Tool | Fact | Status | Citation |
|---|---|---|---|
| VS Code (Workspace Trust) | Storage key `content.trust.model.key`, scope `StorageScope.APPLICATION_SHARED`, value JSON `{ uriTrustInfo: [{ uri, trusted }] }`. A URI is trusted when it is equal to or under a trusted URI (`isEqualOrParent`). | V (source) | `workbench/services/workspaces/common/workspaceTrust.ts` L37, L256-282, L393-395 |
| VS Code (per-server nonce) | Field `trustedAtNonce` in entry keyed by server definition id (for example `cursor-workspace.<folderIndex>.<name>`), inside storage key `mcpToolCache` (LRU of 128). Two caches: `StorageScope.PROFILE` and `StorageScope.WORKSPACE`. Command "MCP: Reset Trust" clears the cache. | V (source); which cache per collection I | `mcpServer.ts` L119-191, L439-445; `mcpService.ts` L56-57, L165-167 |
| Zed | SQLite table `trusted_worktrees (trust_id, absolute_path, user_name, host_name)` in the workspace database. Key: absolute path plus remote user/host. | V (source) | `crates/workspace/src/persistence.rs` L986-991 |
| Cursor | Not documented in official docs. A forum user reports files `.workspace-trusted` and `mcp-approvals` for the CLI (not official; not used as evidence). | U | — |
| Goose | `config.yaml` key `plugins`, map key = plugin root path string, value `{ enabled: bool }`. | V (source) | `discovery.rs` L9, L123-151 |
| OpenCode | No record. | I | — |

### 5. Not trusted / headless; overrides

| Tool | Fact | Status | Citation |
|---|---|---|---|
| VS Code | Restricted mode: "workspace MCP configuration is blocked and these servers don't start." | V (doc) | mcp-servers doc |
| VS Code | Autostart (no user present) calls with `errorOnUserInteraction: true`; an untrusted workspace or an unknown nonce throws `UserInteractionRequiredError('workspaceTrust' / 'serverTrust')`; the server is listed as needing interaction, not started. | V (source) | `mcpService.ts` L144-150; `mcpRegistry.ts` L232-235, L270-273 |
| VS Code | Overrides: `security.workspace.trust.enabled` ("Default is true"; saved setting); `--disable-workspace-trust` ("only affects the current session"); trust of a parent folder covers subfolders. | V (doc) | workspace-trust doc |
| Zed | Untrusted: project settings pending, MCP servers from them not started. Override `"session": { "trust_all_worktrees": true }`. "Auto-trusted worktrees are not persisted between restarts; only manually trusted worktrees are." Command `workspace::ClearTrustedWorktrees`. | V (doc + source) | `docs/src/worktree-trust.md`; `project_settings.rs` L93-98 |
| Cursor | Workspace Trust "is disabled by default." CLI: `--trust` "Trust the workspace without prompting (headless mode only)"; `--approve-mcps` "Automatically approve all MCP servers"; `agent mcp enable <id>` "adds the server to the local approved list". Whether `--approve-mcps` is saved: not documented. | V (doc); persistence U | cursor.com/docs/agent/security; cursor.com/docs/cli/reference/parameters; cursor.com/docs/cli/mcp |
| Goose | No trust state. Opt-out: `disabledPlugins` in `~/.config/goose/settings.json`, `<project>/.config/goose/settings.json`, or `settings.local.json`; or `enabled: false` in `config.yaml`. Plugins are skipped with `--no-profile`, on resume, or with recipe extensions. | V (source + doc) | `discovery.rs` L154-250; `plugins.md` L175-192; `builder.rs` L591 |
| OpenCode | Env var `OPENCODE_DISABLE_PROJECT_CONFIG` skips project `opencode.json` and project `.opencode` dirs. | V (source) | `paths.ts` L27; `config.ts` L420; `packages/core/src/flag/flag.ts` L54-55 |

### 6. Same name in workspace and user configuration

| Tool | Fact | Status | Citation |
|---|---|---|---|
| VS Code | Both are registered (ids carry the collection prefix). Collections sort by `order` ascending; lower is higher priority: `WorkspaceFolder = 0` (`.vscode/mcp.json`), `.mcp.json` = 1, `Workspace = 100`, `User = 200`, `Extension = 300`, `Plugin = 350`, `Filesystem = 400`, `RemoteBoost = -50`. Setting `chat.mcp.collisionBehavior` (default `disable`): "Servers whose label collides with a higher-priority server are disabled by default (unless the user has explicitly toggled them)." Label match is case-insensitive. With `suffix`, both run and tool prefixes get a number suffix. So by default the workspace server wins over the user server. | V (source) | `mcpTypes.ts` L144-153; `mcpRegistry.ts` L130; `mcpService.ts` L53-54, L243-285; `mcpServer.ts` L229-252; PR #312234 |
| Zed | Project settings merge on top of user settings. `context_servers` is a map merged per key; the value type `ContextServerSettingsContent` is an enum, and the derived merge for an enum replaces the whole value (`*self = other.clone()`). So the project entry replaces the user entry of the same name. | V (source) | `crates/settings/src/settings_store.rs` L1454-1461; `crates/settings_content/src/merge_from.rs` L89-103; `crates/settings_macros/src/settings_macros.rs` L61-63; `settings_content/src/project.rs` L506-508 |
| Cursor | Not documented. | U | — |
| Goose | Plugin servers are named `"{plugin_name}:{server_name}"`, so they do not share names with user extensions in normal use. Project plugins sort before user plugins (`Project => 0, User => 1`). | V (source) | `mcp_servers.rs` L134-160; `discovery.rs` L113-118 |
| OpenCode | Deep merge; "Later configs override earlier ones only for conflicting keys." Order: remote, global, `OPENCODE_CONFIG`, project, `.opencode` dirs, `OPENCODE_CONFIG_CONTENT`, managed. Project overrides global per key, and fields merge inside one server entry (remeda `mergeDeep`). | V (doc + source) | `config.mdx` L36-55; `config.ts` L40-51, L412-488 |

### 7. Variable expansion in workspace files

| Tool | Fact | Status | Citation |
|---|---|---|---|
| VS Code `.vscode/mcp.json` | Uses the configuration resolver (`variableReplacement`): predefined variables such as `${workspaceFolder}`, and `${input:id}` ("VS Code prompts you for the value ... The value is then securely stored for subsequent use"). Resolution runs on the serialized launch (all fields) after the trust check. | V (source + doc) | `installedMcpServersDiscovery.ts` L102-106; `mcpRegistry.ts` L462-492, L529-544; mcp-configuration doc |
| VS Code `.mcp.json` | No `variableReplacement`, so no `${input:}` or `${workspaceFolder}`. Only `${VAR}` and `${VAR:-default}` from the host environment: stdio `command`, each `args`, `env` values; HTTP `url` (only if it parses) and header values. Bare `$VAR` stays literal. The Add Server flow rejects `inputs` and any `${...}` for `.mcp.json` ("is not supported in .mcp.json. Use .vscode/mcp.json."). | V (source) | `workspaceDotMcpDiscovery.ts` L93-97; `mcpEnvironmentVariableExpansion.ts` L17-60; `mcpWorkspaceConfiguration.ts` L22-69 |
| Zed | No variable substitution in `context_servers` found. | I (search) | — |
| Cursor | "Cursor resolves variables in these fields: `command`, `args`, `env`, `url`, and `headers`." Syntax: `${env:NAME}`, `${userHome}`, `${workspaceFolder}`, `${workspaceFolderBasename}`, `${pathSeparator}` / `${/}`. `envFile` is STDIO only. No project-vs-global restriction documented. | V (doc) | cursor.com/docs/context/mcp |
| Goose | Only `${PLUGIN_ROOT}` (string replace) in `command`, `args`, `env` values, `cwd`; `PLUGIN_ROOT` is also set in env. | V (source) | `mcp_servers.rs` L13, L134-169 |
| OpenCode | `{env:VAR}` and `{file:path}` are replaced in the raw text of every config file, project or global, before parse. `{file:}` accepts absolute paths and `~/`; relative paths resolve from the config file directory. No restriction for project files. | V (source) | `packages/opencode/src/config/variable.ts` L33-90 |

### 8. Owner, permission, symlink checks

| Tool | Fact | Status | Citation |
|---|---|---|---|
| VS Code | No owner, mode or symlink check found in MCP discovery. | I (search) | `workbench/contrib/mcp/common/discovery/` |
| Zed | No owner or mode check found in trust code. | I (search) | `crates/project/src/trusted_worktrees.rs` |
| Cursor | Not documented. | U | — |
| Goose | Env keys in a deny list are dropped: `PATH`, `LD_PRELOAD`, `LD_LIBRARY_PATH`, `DYLD_INSERT_LIBRARIES`, `NODE_OPTIONS`, `PYTHONPATH`, and 25 more ("Skipping disallowed env var"). Manifest paths must start with `./` and must not contain `..` or be absolute. No owner or symlink check found. | V (source); last part I | `crates/goose/src/agents/extension.rs` L91-150; `open_plugins.rs` L391-412 |
| OpenCode | No owner, mode or symlink check found. | I (search) | `config.ts`, `paths.ts` |

### 9. VS Code PR #260209 / issue #258823

| Fact | Status | Citation |
|---|---|---|
| Issue #258823 "MCP Server auto-start concerns": user saw Copilot start a server from Cursor's config "instead of the one in VS Code". | V | github.com/microsoft/vscode/issues/258823 |
| PR #260209 "mcp: turn other-app discovery off by default", merged 2025-08-07, merge commit `a941a5e28d36689505445de0af9966ab131c724c`. | V | github.com/microsoft/vscode/pull/260209 |
| Setting `chat.mcp.discovery.enabled` changed from `boolean | object` with `default: true` to an object only, `additionalProperties: false`, default every source `false`. | V (diff) | `chat/browser/chat.contribution.ts` in that commit |
| `_isDiscoveryEnabled` changed from "on unless the source is `false`" to "off unless the source is `true`". A migration turns an old boolean into a per-source object with that value. | V (diff) | `nativeMcpDiscoveryAbstract.ts`, `chat.contribution.ts` in that commit |
| Sources: Claude Desktop, Windsurf, Cursor global (`~/.cursor/mcp.json`), Cursor workspace (`.cursor/mcp.json`). Add Server gained "Add from another application...". | V (diff) | `mcpConfiguration.ts`, `mcpCommandsAddConfiguration.ts` in that commit |
| Current state: a Copilot CLI source was added later and is on by default; others stay off. Code comment: "The Copilot CLI user configuration is on by default because MCP server migration moves user servers there; other applications' configurations remain opt-in." | V (source) | `workbench/contrib/mcp/common/mcpConfiguration.ts` L43-54 |

### 10. Cursor CVE-2025-54136 ("MCPoison") and CVE-2025-54135

| Fact | Status | Citation |
|---|---|---|
| CVE-2025-54136 (GHSA-24mc-g4xr-4395, 2025-08-01): approval was bound to the server name. Check Point: "this trust is bound only to the name"; "future modifications to its command or arguments are trusted without any additional validation or prompt." | V | GHSA-24mc-g4xr-4395; Check Point Research |
| Fix: "Cursor now requires user approval every time an mcpServer entry is modified (in addition to the existing behavior of asking for approval when a new server is added)." Affected `< 1.2.4`; patched `1.3`. | V | GHSA-24mc-g4xr-4395 |
| Check Point test of 1.3: "any change to an MCP configuration—including something as small as adding a space—triggers a mandatory approval prompt." "The user must explicitly approve or reject the modified MCP before it can take effect." Release notes did not mention the fix. | V (reporter) | Check Point Research |
| Granularity of a whitespace change (whole file vs entry) and the hash method: not published by Cursor. | U | — |
| CVE-2025-54135 (GHSA-4cxx-hrm3-49rm): the agent could create a missing `.cursor/mcp.json` without approval and so add a server. Fix: "The agent has been blocked from writing MCP-sensitive files without approval." Affected `<= 1.2.1`; patched `1.3.9`. | V | GHSA-4cxx-hrm3-49rm |

---

## Part: Prior art: "a file in a project folder can run code"

Research date: 2026-10-10. Facts only, no recommendations.

### Sources read

| Tool | Source | Version / commit |
|---|---|---|
| direnv | github.com/direnv/direnv (shallow clone) | v2.38.2, commit 28a73d1df3f387e16f3c7fa888c2a049e0059e7a |
| mise | github.com/jdx/mise (shallow clone) | 2026.10.7, commit 4599c53b4286ff101de876f8122feec0797b48b2 |
| git | github.com/git/git (shallow clone) | v2.56 dev (DEF_VER=v2.56.GIT), commit 6de20f6092dcf9bdb1c8efe03db4b70c82b423dd |
| VS Code | github.com/microsoft/vscode (sparse clone + one file by `gh api`) | 1.142.0, commit 959031245ebb1fe077e0d512e1397c0ae82006e4 |
| Nix | github.com/NixOS/nix `src/libflake/config.cc` (by `gh api`) | commit 9bf3582eafaa03e484bd086f6cccc8ea08621460 |
| Cargo | https://doc.rust-lang.org/cargo/reference/config.html | current doc, 2026-10-10 |
| Advisories | `gh api repos/<owner>/<repo>/security-advisories` | 2026-10-10 |

Legend: **V** = verified (read source or official doc). **I** = inferred (not stated directly by a source).

---

### Q1. How the tool finds the project file

| Tool | Behavior | Stop condition | Citation | Status |
|---|---|---|---|---|
| direnv | Upward search from the working directory. Takes the first `.envrc` (and `.env` only if `load_dotenv` is on). Only regular files count. | Filesystem root `/`. No stop at git root, home, filesystem boundary or owner change. | `internal/cmd/rc.go` `findEnvUp`, `findUp`, `eachDir` (walks `path[:i]` to `/`); `fileExists`: "check that the file is a regular file" | V |
| mise | Loads every config file from filesystem root down to the current directory and merges them (deeper wins). Not "first match". | Filesystem root, or below the nearest `ceiling_paths` entry ("files in the ceiling directory itself are not loaded"). No owner or filesystem-boundary stop found. | `docs/configuration.md` "How config files combine": "Config files in every directory from the filesystem root down to the current directory"; `settings.toml` `[ceiling_paths]`: "This matches Git's `GIT_CEILING_DIRECTORIES`." | V (no owner stop: I, by grep for uid/owner in `src/config`) |
| mise `.miserc.toml` | Separate upward search for early settings. | "The search ends after your home directory, or at the filesystem root when you are outside it, and stops before any directory listed in the `MISE_CEILING_PATHS` environment variable". | `docs/configuration.md` "Where mise looks for .miserc.toml" | V |
| git | Upward search. In each directory it tests `.git` file (gitfile), `.git/` dir, then the directory itself as a bare repo. Takes the first hit. | (a) `GIT_CEILING_DIRECTORIES` entry; (b) filesystem boundary (device change) unless `GIT_DISCOVERY_ACROSS_FILESYSTEM`; (c) root. `GIT_DIR` set: no discovery. | `setup.c` `repo_discovery_find_dir`: comment "Test in the following order ... .git (file ...), .git/, ./ (bare), ../.git ..."; returns `GIT_DIR_HIT_CEILING`, `GIT_DIR_HIT_MOUNT_POINT`; die text "Stopping at filesystem boundary (GIT_DISCOVERY_ACROSS_FILESYSTEM not set)." | V |
| VS Code | No upward search. The unit is the opened folder / workspace URI. Trust lookup uses the longest trusted parent in the stored list. | n/a | `workspaceTrust.ts` `doGetUriTrustInfo`: `isEqualOrParent(uri, trustInfo.uri)` with max `fsPath.length` | V |
| Cargo | Upward search for `.cargo/config.toml` from the current directory, merged; then `$CARGO_HOME/config.toml`. | Filesystem root (`/.cargo/config.toml` listed). No trust check. | Cargo book "Hierarchical structure": "It looks for configuration files in the current directory and all parent directories." The page has no trust or ownership section. | V |
| Nix flake `nixConfig` | Per flake, not a directory search. | n/a | `src/libflake/config.cc` `ConfigFile::apply` | V |

### Q2. Approval unit, what is hashed, where records live

| Tool | Approval unit | Hash input | Record location / format | Re-approval on change | Citation | Status |
|---|---|---|---|---|---|---|
| direnv allow | One file, by absolute path **and** content | SHA-256 of `abs_path + "\n" + file bytes` | `$XDG_DATA_HOME/direnv/allow/<sha256 hex>`; file body is the path + `\n`. Default `~/.local/share/direnv/allow` | Yes, any byte change or a move gives a new hash. Old records stay until `direnv prune`. | `rc.go` `fileHash` (`hasher.Write([]byte(path + "\n"))`, `io.Copy(hasher, fd)`); `allow()` writes `path+"\n"` mode 0644; `config.go` `AllowDir() = DataDir/"allow"`; man: "`$XDG_DATA_HOME/direnv/allow`: Records which `.envrc` files have been `direnv allow`ed." Changelog 2.x: "Allow/Deny mechanism now includes the path to make it more secure." | V |
| direnv deny | One file, by path only | SHA-256 of `abs_path + "\n"` | `$XDG_DATA_HOME/direnv/deny/<sha256>`; deny is checked first | No: deny holds across content changes | `rc.go` `pathHash`, `Allowed()` checks `denyPath` before `allowPath` | V |
| direnv `require_allowed` (2.38+) | Extra files that the `.envrc` names (relative, no `..`) | `fileHash` of each file, stored under the `.envrc` path hash | `$XDG_DATA_HOME/direnv/allowed-required/<envrc path hash>/<file hash>` | "If any of the specified files are modified, removed, or new files in the list appear, direnv will require you to run `direnv allow` again" | `cmd_check_required.go`; `man/direnv-stdlib.1.md` `require_allowed` | V |
| direnv `[whitelist]` | Path prefix or exact path in user `direnv.toml` | None | user config | No ("regardless of contents") | `man/direnv.toml.1.md` `[whitelist]`: "**This feature should be used with great care**" | V |
| mise (normal mode) | A **config root directory** (usually the folder that holds `mise.toml`); covers all config files there | No content hash. File name = `<parent 20 chars>-<file 20 chars>-<SipHash of canonical path>` | `$MISE_STATE_DIR/trusted-configs/` (default `$XDG_STATE_HOME/mise`), a symlink (or file) to the canonical path. Declines go to `ignored-configs/`. | No | `config_file/mod.rs` `trust()`, `trust_path()`, `hashed_path_filename()` (uses `hash_to_str` = `SipHasher`, `crates/mise-util/src/hash.rs`); `dirs.rs` `TRUSTED_CONFIGS = STATE.join("trusted-configs")`; `docs/security.md`: "In normal mode, trust belongs to a config root" | V |
| mise (paranoid) | Each file on its own | SHA-256 of file content | Same record plus `<record>.hash` | Yes: "any edit makes it untrusted again" | `trust()` writes `file_hash_sha256` to `.hash` when `paranoid`; `trust_file_hash()` compares; `docs/paranoid.md` | V |
| mise extras | Monorepo root marker trusts all descendants by path; a linked git worktree inherits trust of the same path in the main checkout (normal mode only) | none | `<record>.monorepo` | No | `is_trusted()` monorepo loop; `crate::git::main_checkout_equivalent`; `docs/security.md` "Monorepo roots", "Git worktrees" | V |
| mise trust exemptions | A `mise.toml` with only `min_version`, plain `[tools]` versions and `[tasks]` without `secrets`, and no template syntax, loads without trust. `.tool-versions` needs trust only with templates or inline options. `.miserc.toml` never needs trust. | n/a | n/a | n/a | `docs/security.md` "What needs trust" | V |
| git safe.directory | Repository path (worktree for non-bare, gitdir for bare); value may be exact path, `path/*`, `.`, or `*` | none | `safe.directory` in protected config (system, global, command) | No | `setup.c` `safe_directory_cb`, `ensure_valid_ownership` comment: "for non-bare repositories, their worktree needs to be added, for bare ones their git directory" | V |
| VS Code | Folder or workspace URI; trusting a parent covers subfolders | No content hash found | Storage key `content.trust.model.key`, `StorageScope.APPLICATION_SHARED`, `StorageTarget.MACHINE` | Not stated in docs ("The page doesn't say whether trust is re-requested when files change"); source stores URIs only | `workspaceTrust.ts` lines 37, 256, 282; doc: "You can also trust a parent folder, which applies trust to the parent folder and all its subfolders." | V (no re-approval: I) |
| Nix flake nixConfig | One `(setting name, value)` pair, global across all flakes | none | `~/.local/share/nix/trusted-settings.json` (`getDataDir()/trusted-settings.json`), stores trusted **or** untrusted per value | New value = new prompt (keyed by value string) | `config.cc` `trustedListPath`, `trustedList[name][valueS] = trusted` | V |

### Q3. Owner, permission and symlink checks

| Tool | Check | Exact rule | Walk stops or use fails? | Citation | Status |
|---|---|---|---|---|---|
| git | Owner of: the worktree directory, the git directory, and the `.git` gitfile (if any). Each must have `st_uid == geteuid()`. | If euid is root: accept root-owned paths, else compare with `SUDO_UID`. Uses `lstat` (owner of the link itself, not the target). On failure, accept only if the real path matches a `safe.directory` entry. | **The use fails.** The walk returns at the first `.git` found with `GIT_DIR_INVALID_OWNERSHIP`; it does not continue upward. Then git dies "detected dubious ownership in repository at '...'", or, for `nongit_ok` callers, acts as outside a repo. | `setup.c` `ensure_valid_ownership`, loop `return ret`, `GIT_DIR_INVALID_OWNERSHIP` case; `git-compat-util.h` `is_path_owned_by_current_uid` (`lstat`, `SUDO_UID`) | V |
| git (history) | 2.30.3 (CVE-2022-24765) checked the worktree owner. 2.30.5 (CVE-2022-29187) "is now extended to also cover the ownership of the Git directory (and the `.git` file, if there is any)". 2.39.4 (CVE-2024-32004) extended checks "to cover cloning local repositories". | | | `Documentation/RelNotes/2.30.3.adoc`, `2.30.5.adoc`, `2.39.4.adoc` | V |
| git bare repos | `safe.bareRepository=explicit` refuses implicit bare repos found by discovery; default `all` in Git 2.x, "will be the default in Git 3.0". | | Use fails with "cannot use bare repository" | `Documentation/config/safe.adoc`; `setup.c` `GIT_DIR_DISALLOWED_BARE` | V |
| direnv | No owner or permission check found. Discovery requires a regular file (follows symlinks via `os.Open` + `Stat`). `direnv allow <path>` runs `filepath.EvalSymlinks`; the load path uses `filepath.Abs` only. | | n/a | `rc.go` `fileExists`, `fileHash`; `cmd_allow.go` line 36; grep found no uid/owner code in `internal/cmd` | V (absence: I) |
| direnv | Allow records written with mode 0644 (comment `#nosec G306`). | | | `rc.go` `allow()` | V |
| mise | No owner check found. Trust lookup canonicalizes the path (`path.canonicalize()`, resolves symlinks); a canonicalize error gives "not trusted". | | n/a | `is_trusted()` | V (absence of owner check: I) |
| VS Code | No owner check found in `workspaceTrust.ts`. URIs are canonicalized (remote authority, query/fragment removed). | | | `workspaceTrust.ts` `getCanonicalUri` | I (only one file read) |
| Nix | `trusted-users` is a daemon-side list; "essentially equivalent to giving that user root access". Not a file-owner check. | | | nix manual conf-file `trusted-users` | V |

### Q4. Non-interactive behavior and overrides; can a project set them?

| Tool | Non-interactive behavior | Overrides | Can the project set the override? | Citation | Status |
|---|---|---|---|---|---|
| direnv | Never prompts. Unallowed file: error "%s is blocked. Run `direnv allow` to approve its content"; env not loaded. Denied: silently skipped. | `[whitelist] prefix/exact` in `direnv.toml` (dir from `DIRENV_CONFIG` / XDG config); `DIRENV_DISABLE=1` stops changes. | Whitelist is in the user config dir, not the project. A project `.envrc` can call `source_env` / `source_up`: "the other `.envrc` is not checked by the security framework." | `rc.go` `notAllowed`, `Load()`; `config.go`; `man/direnv.1.md`; `man/direnv-stdlib.1.md` | V |
| mise | Terminal: prompt Yes/No/All; No adds an ignore record. No terminal: fails "Config files in <path> are not trusted". Shell activation (`hook-env`): never asks, skips and warns. | `MISE_TRUSTED_CONFIG_PATHS` (`["/"]` trusts all), `MISE_YES`/`--yes` (answers yes and records trust), `CI` set: "treats every config as trusted and records nothing", `MISE_PARANOID`, `MISE_SAFE`, `MISE_IGNORED_CONFIG_PATHS`, `MISE_CEILING_PATHS`. Implicit trust: `mise run`, `install`, `exec`, `watch`, `daemons start/restart/register` record trust for all non-global configs they load (normal mode, outside CI). `__MISE_REQUIRE_EXPLICIT_TRUST` (set for MCP server children) turns off implicit and `yes` trust. | `trusted_config_paths`, `yes`, `paranoid`, `safe` are `global_only = true`. A project `.miserc.toml` (no trust needed) **can** set `ceiling_paths`, `ignored_config_paths`, `env` and override filenames. Before 2026.6.4 a project `[settings]` could set `trusted_config_paths`/`yes` (CVE-2026-35533). | `config_file/mod.rs` `trust_check`, `is_trusted` ("in tests/CI we trust everything"), `REQUIRE_EXPLICIT_TRUST_ENV`; `docs/security.md` "When mise asks", "Implicit trust", "CI and --yes"; `settings.toml`; `docs/configuration.md` miserc keys | V |
| git | No prompt. Dies or treats as non-repo. | `safe.directory` (path, `path/*`, `.`, `*`); empty value resets list; `GIT_CEILING_DIRECTORIES`; `GIT_DISCOVERY_ACROSS_FILESYSTEM`; `GIT_DIR`; `-c` / `GIT_CONFIG_COUNT/KEY/VALUE` count as protected "command" scope. `GIT_TEST_ASSUME_DIFFERENT_OWNER` (test only). | No. `safe.directory` and `safe.bareRepository` are "only respected in protected configuration ... This prevents untrusted repositories from tampering with this value." Protected = system, global, command scopes. | `Documentation/config/safe.adoc`; `Documentation/git-config.adoc` "Protected configuration"; `config.c` `read_protected_config` (`ignore_repo = 1`, `ignore_worktree = 1`); `Documentation/git.adoc` env vars | V |
| VS Code | Untrusted folder opens in Restricted Mode; `security.workspace.trust.startupPrompt` default `never` (banner, no dialog). Tasks/debug ask for trust when started. | `security.workspace.trust.enabled`, `.startupPrompt`, `.untrustedFiles`, `.emptyWindow`; `--disable-workspace-trust` ("only affects the current session"); a remote resolver can set `isTrusted`. | No: these settings have `scope: ConfigurationScope.APPLICATION` = "can only be configured in user settings". | `workspace.contribution.ts` lines 799-850 (commit above); VS Code contribution-points doc; workspace-trust doc; `workspaceTrust.ts` `isWorkspaceTrustEnabled` | V |
| Nix | `logger->ask(...).value_or('n')`: with no answer the setting is not trusted; warns "ignoring untrusted flake configuration setting ... Pass '--accept-flake-config' to trust it". | `accept-flake-config` (default false). Whitelist without prompt: `bash-prompt`, `bash-prompt-prefix`, `bash-prompt-suffix`, `flake-registry`, `commit-lock-file-summary`. | The flake cannot set `accept-flake-config` for itself (I: it is checked from `flakeSettings` before applying flake values). | `config.cc` `ConfigFile::apply` | V (project-cannot-set: I) |
| Cargo | No approval step. | `--config`, `CARGO_HOME` | n/a | Cargo book | V |

### Q5. Known bypasses / CVEs and fixes

| Tool | ID | Issue | Fix | Citation | Status |
|---|---|---|---|---|---|
| git | CVE-2022-24765 | Another user creates `.git` in a shared parent (e.g. `C:\.git`); a Git-aware prompt or editor then runs that user's config commands. | 2.30.3 (and other maint lines): worktree owner check + `safe.directory`. 2.30.4: key-name check fix, `*` value. | `RelNotes/2.30.3.adoc`, `2.30.4.adoc` | V |
| git | CVE-2022-29187 | Check covered only the worktree. | 2.30.5: also check the git directory and the `.git` file. | `RelNotes/2.30.5.adoc` | V |
| git | CVE-2024-32004 | Local clone of another user's repo runs code. | 2.39.4 / 2.45.1: ownership checks extended to local clones. | `RelNotes/2.39.4.adoc` | V |
| git | CVE-2024-32020 | Local clone hardlinks objects from a repo of another user. | same releases | `RelNotes/2.39.4.adoc` | V |
| direnv | none | GitHub advisory list for direnv/direnv is empty; web search found no CVE. | — | `gh api repos/direnv/direnv/security-advisories` returned `[]` | V (absence is limited to these two searches) |
| direnv | by design | `source_env`, `source_up`, `source_up_if_exists` load other files that are "not checked by the security framework"; `[whitelist]` allows "regardless of contents". | documented, not a bug | `man/direnv-stdlib.1.md`, `man/direnv.toml.1.md` | V |
| direnv | possible TOCTOU | The hash is computed in Go, then bash reads the file again by path. A change between the two is not detected. | not reported | `rc.go` `RCFromPath` then `Load()` runs `bash -c ... source_env <path>` | I |
| mise | CVE-2026-33646 (critical) | `.tool-versions` rendered with Tera `exec()` without trust check. | 2026.3.10 | GHSA-fjj5-v948-whjj | V |
| mise | CVE-2026-35533 (high) | Project `[settings]` loaded before trust check; `trusted_config_paths = ["/"]` or `yes`/`ci` made the file trust itself. | 2026.6.4: `global_only` settings stripped from non-global config | GHSA-436v-8fw5-4mj8 | V |
| mise | CVE-2026-55448 | Project `github.credential_command` ran before trust. | 2026.6.4 | GHSA-29hf-rm4x-xxph | V |
| mise | CVE-2026-55441 (high) | Task-include dirs in a repo with no config file never reached `trust_check`; listing tasks ran `exec()`. | 2026.6.4 | GHSA-77g9-363w-rccq | V |
| mise | CVE-2026-103929 (high) | Shell-interpreter args settings missing from the `global_only` denylist. | 2026.7.14 | GHSA-g74g-rg72-j2p3 | V |
| mise | GHSA-6986-cq7v-7cj9 (high) | `history.describe_command` not `global_only`; implicit trust from `mise install/exec/run` made it reachable. | v2026.9.7 | advisory | V |
| mise | GHSA-wcqh-j26q-g44x | `.tool-versions` trust gate looked only for template delimiters; inline `[api_url=...]` leaked `GITHUB_TOKEN`. | 2026.10.0 | advisory | V |
| mise | GHSA-333c-h2jr-xv83 | Project-controlled GitHub `api_url` exfiltrated `GITHUB_TOKEN`. | (not read) | advisory list | V (summary only) |
| VS Code | CVE-2021-34529 | Addressed by introducing Workspace Trust (1.57). | 1.57.1 notes | web search result citing code.visualstudio.com/updates/v1_57 | I (not read directly) |
| VS Code | CVE-2022-30129 | Trust bypass when the last focused window was trusted (git URL handler). | 1.67.1 | secondary source (wiz.io), not MSRC | I |
| VS Code | CVE-2026-69278, CVE-2026-58650 | MSRC CSAF: Workspace Trust bypass. | details not read | msrc.microsoft.com CSAF URLs from search | I |
| Nix | none checked | Not researched. | — | — | not verified |
| Cargo | none found for config discovery | Not researched in depth. | — | — | not verified |

### Cross-tool facts (from the tables)

- Content-bound approval: direnv (path + content, always), mise paranoid (content per file), Nix (per setting value). Path-only approval: mise normal mode, VS Code, git `safe.directory`, direnv whitelist.
- Owner check: only git, and it fails the use; it does not skip to a higher directory.
- Overrides that widen trust live only in user-level config or the environment for git, VS Code, direnv and mise (after the 2026.6.4 fix). mise CVE history shows that a project file that could set its own trust settings defeated the gate.
- mise auto-trusts in CI (`CI` set) unless paranoid; direnv and git never auto-approve.
