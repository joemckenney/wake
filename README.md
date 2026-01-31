<p align="center">
  <img src="assets/logo.svg" alt="wake" width="80" height="80">
</p>

<h1 align="center">wake</h1>

<p align="center"><strong>The trail you leave behind.</strong></p>

Wake records your terminal sessions—commands, outputs, git context—so Claude Code can see what you've been doing.

## Installation

```sh
curl -sSf https://raw.githubusercontent.com/joemckenney/wake/main/install.sh | sh
```

## Setup

Add to `~/.zshrc` or `~/.bashrc`:

```sh
eval "$(wake init zsh)"   # or: wake init bash
```

Add the MCP server to Claude Code:

```sh
claude mcp add --transport stdio --scope user wake-mcp -- wake-mcp
```

## Usage

### The Workflow

1. **Start** — `wake shell`
2. **Work** — builds, deploys, debugging
3. **Ask Claude** — it sees your terminal history, no copy-pasting

### Example

```
$ wake shell
$ kubectl logs deploy/api-server | tail -100
  # wall of errors
$ kubectl describe pod api-server-7f8b9
  # more stuff you don't have time to read
```

> **You:** Summarize what's happening with the API server
>
> **Claude:** The API server is crash-looping. From the logs, it's failing to connect to Redis on startup—connection refused to `redis:6379`. The pod events show 5 restarts in the last 10 minutes. Looks like the Redis service might be down.

### CLI Reference

```sh
wake shell              # Start recorded session
wake log                # Recent commands
wake search "error"     # Search history
wake dump               # Export session as markdown
wake annotate "note"    # Add a breadcrumb for context
wake prune              # Delete old sessions
wake prune --dry-run    # Preview what would be deleted
wake prune --force      # Skip confirmation
wake prune --older-than 7  # Override retention period (days)
wake llm status         # Check model status (downloaded/loaded)
wake llm download       # Pre-download model (optional, auto-downloads on first use)
```

### MCP Tools

Claude Code uses these tools via the MCP server:

| Tool | Purpose |
|------|---------|
| `wake_status` | Current session info |
| `wake_list_commands` | List recent commands with metadata and summaries (no full output) |
| `wake_get_output` | Fetch full output for specific command IDs |
| `wake_log` | Recent commands with truncated output |
| `wake_search` | Search command history |
| `wake_dump` | Export session as markdown |
| `wake_annotate` | Add notes to the session |

The `wake_list_commands` + `wake_get_output` pattern enables **tiered retrieval**—Claude sees command metadata first, then fetches full output only when needed. This reduces context usage for long sessions.

### Configuration

Create `~/.wake/config.toml`:

```toml
[retention]
days = 21      # Delete sessions older than this (default: 21)

[output]
max_mb = 5     # Max output size per command in MB (default: 5)

[summarization]
enabled = false  # Disable LLM summarization (default: true)
min_bytes = 500  # Minimum output size to trigger summarization (default: 1024)
```

Or use environment variables (take precedence over config file):

```sh
export WAKE_RETENTION_DAYS=14
export WAKE_MAX_OUTPUT_MB=10
```

Old sessions are automatically pruned on each `wake shell` start.

### LLM Summarization

Wake automatically summarizes command outputs using a local LLM (Qwen3-0.6B). Summaries appear in `wake_list_commands` output, helping Claude quickly understand what happened without reading full output.

**Enabled by default.** On first run, the model (~380MB) downloads automatically.

- **CPU-friendly** — The small model runs efficiently without a GPU
- **Privacy** — All inference happens locally, nothing leaves your machine
- **Manual download** — `wake llm download` to pre-download the model
- **Disable** — Set `enabled = false` in config if you don't want summarization

**How it works:**
1. When a command completes with output > `min_bytes`, it's queued for summarization
2. A background task runs inference on the local model
3. Summaries are stored in the database and exposed via MCP tools

**GPU acceleration (build from source):**

The default build and pre-built binaries use CPU inference, which is fast enough for the small model. For faster inference on supported hardware, build with GPU features:

```sh
cargo build --release --features cuda   # NVIDIA (requires CUDA toolkit)
cargo build --release --features metal  # Apple Silicon
```

## How It Works

```
┌─────────────────────────────────────────────────────────────────────┐
│                            wake shell                               │
│                                                                     │
│  ┌───────────┐       ┌─────────────┐      ┌──────────────────────┐  │
│  │   Your    │  pty  │    Shell    │ hook │    Unix Socket       │  │
│  │  Terminal │◄─────►│  (zsh/bash) │─────►│  /tmp/wake-*.sock    │  │
│  └───────────┘       └─────────────┘      └──────────┬───────────┘  │
│        │                   │                         │              │
│        │                   │ stdout                  │ cmd events   │
│        │                   ▼                         ▼              │
│        │              ┌────────────────────────────────┐            │
│        │              │         Output Buffer          │            │
│        │              └───────────────┬────────────────┘            │
│        │                              │                             │
│        │                              ▼                             │
│        │                       ┌─────────────┐    ┌──────────────┐  │
│        │                       │  SQLite DB  │◄───│ LLM Summary  │  │
│        │                       │  ~/.wake/   │    │ (background) │  │
│        │                       └─────────────┘    └──────────────┘  │
└────────┼────────────────────────────────────────────────────────────┘
         │                              ▲
         │ you                          │ reads
         ▼                              │
┌──────────────┐               ┌──────────────┐          ┌───────────┐
│   Human at   │               │   wake-mcp   │   mcp    │  Claude   │
│   Keyboard   │               │  MCP Server  │◄────────►│   Code    │
└──────────────┘               └──────────────┘          └───────────┘
```

### Components

| Component    | Purpose                                                        |
| ------------ | -------------------------------------------------------------- |
| `wake shell` | Spawns a PTY, captures all I/O, listens for hook events        |
| Shell hooks  | Installed via `wake init`, notify wake when commands start/end |
| Unix socket  | IPC between shell hooks and the wake process                   |
| SQLite DB    | Stores sessions, commands, outputs, annotations, summaries     |
| LLM engine   | Background task that summarizes command outputs locally        |
| `wake-mcp`   | MCP server that exposes wake data to Claude Code               |

### Data Flow

1. `wake shell` spawns your shell inside a PTY and sets `$WAKE_SESSION`
2. Shell hooks fire on each command, sending metadata via Unix socket
3. PTY output is captured and associated with the current command
4. On command completion, exit code + output are written to SQLite
5. Large outputs are queued for LLM summarization in the background
6. Claude Code queries `wake-mcp`, which reads from the database

### Constraints

- **One session per shell** — Each `wake shell` creates an isolated session
- **Output truncation** — Commands with >5MB output are truncated (configurable)
- **Auto-cleanup** — Sessions older than 21 days are automatically deleted (configurable)
- **Local only** — All data stays in `~/.wake/`, nothing leaves your machine
- **Shell support** — Hooks work with zsh and bash (fish/other shells not yet supported)

## Building from Source

```sh
git clone https://github.com/joemckenney/wake
cd wake
cargo build --release

# With GPU acceleration (optional)
cargo build --release --features cuda   # NVIDIA
cargo build --release --features metal  # Apple Silicon
```

## License

MIT
