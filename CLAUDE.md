# Wake

Terminal session recorder for AI-assisted development.

## Architecture

- **wake-core** - Database, config, manifest, protocol types
- **wake-llm** - Local LLM summarization (Qwen3-0.6B via llama.cpp)
- **wake-cli** - CLI binary (`wake`)
- **wake-mcp** - MCP server binary (`wake-mcp`)

## Development

- `cargo build --release` - Build binaries
- `cargo test --all` - Run all tests
- `cargo clippy --all-targets -- -D warnings` - Lint (CI enforces this)
- `cargo fmt --all` - Format code

## Adding a CLI Command

Use `/add-cli-command` for guided implementation.

## Conventions

- Use `anyhow::Result` for CLI commands
- Use `thiserror` for library error types
- Model files go in `~/.wake/models/`, config in `~/.wake/config.toml`
- Workspace dependencies defined in root `Cargo.toml`
