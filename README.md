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

1. Run `wake shell` to start a recorded session
2. Work normally—everything is captured
3. Claude Code queries your history via MCP when you need help

All data stays local in `~/.local/share/wake/`.

## Building from Source

```sh
git clone https://github.com/joemckenney/wake
cd wake
cargo build --release
```

## License

MIT
