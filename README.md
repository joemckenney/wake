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

Add to `~/.zshrc` or `~/.bashrc`:

```sh
eval "$(wake init zsh)"   # or: wake init bash
```

## Claude Code Setup

Add to `~/.config/claude-code/mcp.json`:

```json
{
  "mcpServers": {
    "wake": {
      "command": "wake-mcp"
    }
  }
}
```

Now Claude can query your terminal history directly—ask *"What did I just run?"* or *"Why did my build fail?"*

## Usage

```sh
wake shell              # Start recorded session
wake log                # Recent commands
wake search "error"     # Search history
wake dump               # Export as markdown
wake annotate "note"    # Add a breadcrumb
```

## How It Works

```
┌─────────────────────────────────────────────────────────────────┐
│                         wake shell                              │
│  ┌───────────┐      ┌─────────────┐      ┌──────────────────┐  │
│  │  Your     │ pty  │   Shell     │ hook │  Unix Socket     │  │
│  │  Terminal │◄────►│  (zsh/bash) │─────►│  /tmp/wake-*.sock│  │
│  └───────────┘      └─────────────┘      └────────┬─────────┘  │
│                                                   │             │
│       stdout captured ─────────────────────────►  │             │
│                                                   ▼             │
│                                          ┌──────────────┐       │
│                                          │   SQLite DB  │       │
│                                          │ ~/.wake/     │       │
│                                          └──────┬───────┘       │
└─────────────────────────────────────────────────┼───────────────┘
                                                  │
┌─────────────────────────────────────────────────┼───────────────┐
│                       wake-mcp                  │               │
│  ┌─────────────┐      ┌─────────────┐           │               │
│  │ Claude Code │ mcp  │ MCP Server  │───────────┘               │
│  │             │◄────►│             │  reads                    │
│  └─────────────┘      └─────────────┘                           │
└─────────────────────────────────────────────────────────────────┘
```

### Components

| Component | Purpose |
|-----------|---------|
| `wake shell` | Spawns a PTY, captures all I/O, listens for hook events |
| Shell hooks | Installed via `wake init`, notify wake when commands start/end |
| Unix socket | IPC between shell hooks and the wake process |
| SQLite DB | Stores sessions, commands, outputs, annotations |
| `wake-mcp` | MCP server that exposes wake data to Claude Code |

### Data Flow

1. `wake shell` spawns your shell inside a PTY and sets `$WAKE_SESSION`
2. Shell hooks fire on each command, sending metadata via Unix socket
3. PTY output is captured and associated with the current command
4. On command completion, exit code + output are written to SQLite
5. Claude Code queries `wake-mcp`, which reads from the database

### Constraints

- **One session per shell** — Each `wake shell` creates an isolated session
- **Output truncation** — Commands with >1MB output are truncated to prevent bloat
- **Local only** — All data stays in `~/.wake/`, nothing leaves your machine
- **Shell support** — Hooks work with zsh and bash (fish/other shells not yet supported)

## Building from Source

```sh
git clone https://github.com/joemckenney/wake
cd wake
cargo build --release
```

## License

MIT
