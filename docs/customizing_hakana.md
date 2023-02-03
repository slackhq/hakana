# Customizing Hakana

You can customize some of Hakana's behaviour with plugins.

Once you've [created your plugin](authoring_plugins.md) you can include them in a custom build.

Here's what our custom Hakana build looks like at Slack — we load the open-source version of Hakana as a git submodule.

Cargo.toml:

```toml
[package]
name = "hakana-custom"
version = "0.1.0"
edition = "2021"
build = "build.rs"

[dependencies]
hakana-cli = { path = "hakana-core/src/cli" }
hakana-analyzer = { path = "hakana-core/src/analyzer" }
```

main.rs

```rust
fn main() {
    hakana_cli::init(
        vec![
            // List analysis hooks here
        ],
        vec![
            // List migration hooks here
        ],
        "My custom Hakana build",
        Box::new( /* custom test runner goes here */ ),
    );
}
```

