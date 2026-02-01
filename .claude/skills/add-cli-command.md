# Add CLI Command

Guide for adding a new command to the wake CLI.

## Steps

1. **Create command file**: `crates/wake-cli/src/commands/<name>.rs`
   - Use `anyhow::Result` for the return type
   - Follow existing command patterns (see `status.rs` or `prune.rs` for examples)

2. **Register module**: Add `pub mod <name>;` to `crates/wake-cli/src/commands/mod.rs`

3. **Add to Commands enum** in `crates/wake-cli/src/main.rs`:
   ```rust
   /// Description of the command
   CommandName {
       #[arg(long)]
       some_flag: bool,
   },
   ```

4. **Add match arm** in `main()`:
   ```rust
   Commands::CommandName { some_flag } => commands::name::run(some_flag).await,
   ```

5. **Update README.md**: Add the command to the CLI Reference section (~line 55)

6. **Run checks**:
   ```sh
   cargo fmt --all
   cargo clippy --all-targets -- -D warnings
   cargo test --all
   ```

## Checklist

- [ ] Command file created with async fn
- [ ] Module registered in mod.rs
- [ ] Enum variant added with doc comment
- [ ] Match arm added in main()
- [ ] README CLI Reference updated
- [ ] All checks pass
