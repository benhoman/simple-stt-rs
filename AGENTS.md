## AGENTS.md

### Build/Lint/Test Commands
- **Build**: `cargo build --release`
- **Lint**: `cargo clippy -- -D warnings`
- **Test**: `cargo test -- --test-threads=1`
- **Single test**: `cargo test -- --test-threads=1 --test <test_name>`

### Code Style Guidelines
- **Imports**: Use absolute paths, group by crate
- **Formatting**: `rustfmt` (4 spaces, no trailing spaces)
- **Types**: Prefer concrete types, use `Result` for errors
- **Naming**: snake_case for variables, PascalCase for types
- **Error Handling**: Return `Result`, use `?` operator
- **Structs**: Public fields, snake_case
- **Enums**: UpperCamelCase, `#[derive(Debug)]`

### Rules
- Cursor rules: [.cursor/rules/](./.cursor/rules/)
- Copilot instructions: [.github/copilot-instructions.md](./.github/copilot-instructions.md)
