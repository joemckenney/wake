<p align="center">
  <pre align="center">
    ╲     ╱
     ╲   ╱
      ╲ ╱
       V

     wake</pre>
  <strong>The trail you leave behind.</strong>
</p>

Every terminal command leaves a trace. Wake captures that trail—commands, outputs, directories, git context—so when you need to retrace your steps or bring an AI assistant up to speed, the context is already there.

## Why wake?

When working with AI coding assistants, context is everything. But by the time you need help, you've already scrolled past the error, forgotten the exact command, or lost track of what you tried.

Wake runs quietly in the background, preserving everything. When you need to look back—or bring someone else into the picture—your entire session is there: what you ran, where you ran it, and what happened.

## Features

- **Transparent recording** — Works with your existing shell (zsh/bash), no workflow changes
- **Rich context** — Captures commands, outputs, exit codes, timing, git branch, and working directory
- **MCP integration** — Claude Code can query your session via the Model Context Protocol
- **Search & export** — Find past commands or export full sessions as markdown
- **Annotations** — Add notes to mark important moments in your session
- **Privacy-first** — All data stays local in `~/.local/share/wake/`

## Installation

```sh
curl -sSf https://raw.githubusercontent.com/joemckenney/wake/main/install.sh | sh
```

Or with Cargo:

```sh
cargo install wake-cli wake-mcp
```

### Shell Setup

Add to your `~/.zshrc` or `~/.bashrc`:

```sh
eval "$(wake init zsh)"   # or: wake init bash
```

## Quick Start

```sh
# Start a recorded session
wake shell

# Work normally — everything is recorded
cargo build
npm test
git status

# View recent commands
wake log

# Search history
wake search "error"

# Export session for sharing
wake dump > session.md
```

## Usage

### Commands

| Command | Description |
|---------|-------------|
| `wake shell` | Start a recorded shell session |
| `wake status` | Show current session info |
| `wake log [-c N]` | Show last N commands (default: 10) |
| `wake search <query>` | Search command history and output |
| `wake dump` | Export session as markdown |
| `wake annotate <note>` | Add a note to the current session |
| `wake init <shell>` | Print shell integration script |

### Examples

View the last 20 commands:
```sh
wake log -c 20
```

Find all failed commands:
```sh
wake search "exit code"
```

Add context for your AI assistant:
```sh
wake annotate "Starting work on authentication feature"
```

## Claude Code Integration

Wake includes an MCP server that lets Claude Code access your terminal history directly.

### Setup

Add to your Claude Code MCP config (`~/.config/claude-code/mcp.json`):

```json
{
  "mcpServers": {
    "wake": {
      "command": "wake-mcp"
    }
  }
}
```

### Available Tools

Once configured, Claude Code can use these tools:

| Tool | Description |
|------|-------------|
| `wake_status` | Get current session info |
| `wake_log` | Retrieve recent commands with output |
| `wake_search` | Search command history |
| `wake_dump` | Export full session as markdown |
| `wake_annotate` | Add notes to the session |

### Example Workflow

1. Start a wake session: `wake shell`
2. Work on your project, encounter an issue
3. Ask Claude Code: *"What commands did I just run?"* or *"Why did my build fail?"*
4. Claude queries your terminal history and provides contextual help

## Data Storage

Wake stores session data in `~/.local/share/wake/wake.db` (SQLite).

View database location:
```sh
echo ~/.local/share/wake/
```

## Building from Source

```sh
git clone https://github.com/joemckenney/wake
cd wake
cargo build --release
```

Binaries will be in `target/release/`:
- `wake` — CLI tool
- `wake-mcp` — MCP server

## License

MIT
