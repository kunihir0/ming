You are a senior Rust engineer. Write idiomatic, production-grade Rust. No shortcuts,
no fighting the borrow checker — work with it.

## Project Layout

```
project/
├── Cargo.toml                  # workspace root only — no [package]
├── Cargo.lock                  # commit this
├── rustfmt.toml
├── .clippy.toml
├── .cargo/config.toml
└── crates/
    ├── bot/            # binary — owns runtime, commands, config
    └── push-receiver/          # library — GCM/FCM, no Discord knowledge
```

## Non-Negotiables

- `cargo fmt --all` before every commit. CI fails on diff.
- `cargo clippy --workspace --all-targets -- -D warnings -D clippy::pedantic` must be clean.
- Zero `unwrap()` in library code. Zero.
- Every public item in a library crate has a `///` doc comment.
- `Cargo.lock` is committed — this is an application workspace.

## Workspace Dependencies

All shared deps live in `[workspace.dependencies]`. Member crates inherit with
`dep.workspace = true`. No version duplication, no drift.

```toml
# Cargo.toml (root)
[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }

# crates/push-receiver/Cargo.toml
[dependencies]
tokio.workspace = true
serde.workspace = true
```

When `cargo add` writes a new dep, immediately promote it to the workspace manifest.

## Error Handling

**Library crates (`push-receiver`)** — typed errors with `thiserror`:

```rust
// Always define a crate-level Result alias
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Registration failed: {0}")]
    Registration(String),
}
```

**Binary crates (`bot`)** — `anyhow` for application-level propagation:

```rust
// main returns anyhow::Result
#[tokio::main]
async fn main() -> anyhow::Result<()> { ... }

// Commands use the poise Error type alias
pub type Error = Box<dyn std::error::Error + Send + Sync>;
```

Rules:
- Never `.unwrap()` in library code. Use `?` or return an `Error`.
- `.expect("reason")` is acceptable in `main` for true preconditions (missing env vars).
- Never silently swallow errors with `let _ = ...` unless explicitly intentional — add a comment.
- Prefer `?` over `match` for propagation. Reserve `match` for when you handle branches differently.

## Async

- Runtime is `tokio`. Library crates must not call `#[tokio::main]` or `Runtime::new`.
- Library async functions are runtime-agnostic — they use `async fn` and `.await`, nothing more.
- Prefer `tokio::select!` for racing futures. Use `JoinSet` for structured concurrency over raw `spawn`.
- Do not hold a `MutexGuard` across an `.await` point. Use `tokio::sync::Mutex` where async locking is needed, `std::sync::Mutex` everywhere else.
- If a function is not async, don't make it async.

```rust
// Wrong — spawns unstructured, errors are lost
tokio::spawn(do_thing());

// Right — structured, errors surface
let mut set = JoinSet::new();
set.spawn(do_thing());
while let Some(res) = set.join_next().await {
    res??;
}
```

## Ownership & Types

- Prefer `&str` over `&String`, `&[T]` over `&Vec<T>` in function signatures.
- Use `impl Into<String>` for owned string params in constructors.
- Reach for `Arc<T>` when shared ownership is genuinely needed. If you find yourself cloning
  `Arc` everywhere, rethink the data model.
- Newtype pattern over type aliases for domain types — `struct UserId(u64)` not `type UserId = u64`.
- Make illegal states unrepresentable. Encode invariants in the type system, not runtime checks.

```rust
// Don't do this
fn process(status: String) { ... }

// Do this
enum Status { Active, Inactive }
fn process(status: Status) { ... }
```

## Structs & Builders

For structs with more than ~3 optional fields, use the builder pattern:

```rust
pub struct PushReceiver {
    sender_id: String,
    http: reqwest::Client,
    timeout: Duration,
}

impl PushReceiver {
    pub fn builder(sender_id: impl Into<String>) -> PushReceiverBuilder {
        PushReceiverBuilder::new(sender_id)
    }
}
```

Derive `Debug` on every struct and enum. Derive `Clone` when it makes sense.
Derive `Copy` only for small value types.

## Traits

- Implement `std` traits where appropriate: `Display`, `From`, `TryFrom`, `Iterator`, `Default`.
- Prefer `impl Trait` in return position for single concrete types.
- Use `dyn Trait` only when you genuinely need runtime polymorphism.
- Keep traits small and focused — prefer multiple small traits over one large one.

## Logging

All crates use `tracing`. Never use `println!` or `eprintln!` for diagnostics.

```rust
// Wrong
println!("Connected: {}", id);

// Right — structured fields, not string formatting
tracing::info!(connection_id = %id, "Connected");

// Spans for operations with duration
let _span = tracing::info_span!("register", sender_id = %self.sender_id).entered();
```

Library crates only emit spans/events. Only the binary configures the subscriber.

## Testing

```rust
// Unit tests in the same file as the code
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_message() { ... }

    #[tokio::test]
    async fn register_fails_without_credentials() { ... }
}

// Integration tests in crates/<name>/tests/
```

- Test the public API, not internals.
- Use `tokio::test` for async tests, not a hand-rolled runtime.
- Name tests as sentences: `parses_valid_message`, `returns_error_on_timeout`.
- Don't mock what you don't own. Wrap external clients behind a trait and mock the trait.

## Adding a Bot Command

1. Create `crates/bot/src/commands/<name>.rs`
2. Write the handler with `#[poise::command(slash_command)]`
3. Export from `commands/mod.rs`
4. Register in the `commands: vec![...]` in `main.rs`

```rust
/// A brief description shown in Discord's command UI.
#[poise::command(slash_command)]
pub async fn my_command(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say("response").await?;
    Ok(())
}
```

## push-receiver Implementation Notes

The crate is intentionally GCM/FCM-agnostic at the module level. No Discord types, no bot
concepts. The FCM connection is three stages — keep them in separate modules:

1. `checkin` — POST to GCM checkin endpoint, get `android_id` + `security_token`
2. `register` — POST to FCM register endpoint, get a registration token
3. `mcs` — Persistent TLS connection to `mtalk.google.com:5228`, MCS protobuf protocol

`client.rs` is the public facade that sequences these stages. Internals are `pub(crate)`.

## What Not To Do

- Don't clone to satisfy the borrow checker — understand why the borrow fails first.
- Don't reach for `unsafe` until you have exhausted safe alternatives.
- Don't use `std::sync::Mutex` inside async code — use `tokio::sync::Mutex`.
- Don't put business logic in `main.rs` — it wires things together only.
- Don't add a dependency for something the standard library already provides well.
- Don't write `return x;` at the end of a function — use expression syntax.
- Don't suppress clippy lints without a comment explaining why.

# Comments Policy
- Only write comments that explain *why* a non-obvious decision was made, not *what* the code does.
- Do not litter the codebase with `TODO` comments — track outstanding work in issues instead.