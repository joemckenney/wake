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

1. Create `crates/wake-cli/src/commands/<name>.rs`
2. Add `pub mod <name>;` to `commands/mod.rs`
3. Add variant to `Commands` enum in `main.rs`
4. Add match arm in `main()` to call the command

## Conventions

- Use `anyhow::Result` for CLI commands
- Use `thiserror` for library error types
- Model files go in `~/.wake/models/`, config in `~/.wake/config.toml`
- Workspace dependencies defined in root `Cargo.toml`
