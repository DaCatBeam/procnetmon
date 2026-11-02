# Agent instructions

## Project
- Rust 2024 TUI that charts Linux network counters from `/proc/net/dev` (ratatui + crossterm).
- Linux-only at runtime. On macOS: `cargo test` runs everything except Linux-only tests;
  run those with `make test-linux` (Docker).
- `src/parse.rs` and `src/model.rs` are the new core; `src/app/` is the old app, being replaced.

## Commands
- Format: `cargo fmt`
- Lint: `cargo clippy --all-targets -- -W clippy::pedantic`
- Test: `cargo test`

## Questions in code (`AGENT_QUESTION:` comments)
- A `///`, `//!`, `//`, or `/* */` comment whose text starts with `AGENT_QUESTION:` is a question about the code
  directly below it. Doc comments () never count.
- Each `// AGENT_QUESTION:` line is its own question. A `/* AGENT_QUESTION: … */` block is one question.
- Answer it. Don't change code because of it, and don't delete it.
- In a review, answer every `AGENT_QUESTION:` comment in the reviewed files in a table at the end,
  before any appendices: `| # | Location | Question | Answer |`. Outside reviews, answer in chat.

- Examples:
```rust
// AGENT_QUESTION: Should we use &str here instead of String for original_potato?
fn potatify(original_potato: String) -> String {
    original_potato + " super potato!!!"
}

/*
AGENT_QUESTION: This is a multiline comment, all lines until the comment terminal syntax
are part of this question.
*/
fn potatify(original_potato: String) -> String {
    original_potato + " super potato!!!"
}

// AGENT_QUESTION: This is one question.
// AGENT_QUESTION: This is a separate question to answer
fn potatify(original_potato: String) -> String {
    original_potato + " super potato!!!"
}

// This is just a normal comment, you can reference it but it doesn't require an answer from you.
fn potatify(original_potato: String) -> String {
    original_potato + " super potato!!!"
}
```

## Instructions in code (`AGENT_INSTRUCTION:` comments)
- A `///`, `//!`, `//`, or `/* */` comment whose text starts with `AGENT_INSTRUCTION:` is a question about the code
  directly below it. Doc comments () never count.
- Each `// AGENT_INSTRUCTION:` line is its own instruction. A `/* AGENT_INSTRUCTION: … */` block is one instruction.
- Only implement it if the instruction is clear. Ambiguitity should prompt a question to the user.
- If the instruction conflicts with any information you received in the initial prompt, prioritize the prompt over the instruction's commands.
- Examples:
```rust
// Before:
// AGENT_INSTRUCTION: Provide documentation for this function.
pub fn potatify(original_potato: String) -> String {
    original_potato + " super potato!!!"
}

// After:
// potatify takes in a string and appends " super potato!!!" to it, returning a newly allocated string.
pub fn potatify(original_potato: String) -> String {
    original_potato + " super potato!!!"
}
```

## Reviews
- Write the review as a markdown file at the repo root, and change nothing else.
- For each finding: location, why it matters, alternatives with code samples, and the
  tradeoffs of each alternative.
- Prefer specific criticism over praise; nitpicks are welcome.
