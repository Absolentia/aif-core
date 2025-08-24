
# Contributing to AIF Core

Thanks for your interest in contributing!

## License and DCO
- This project is dual-licensed: **MIT OR Apache-2.0**.
- We use the **Developer Certificate of Origin (DCO)**. Please sign your commits:
  ```bash
  git commit -s -m "Your message"
  ```

See DCO for details.

Coding standards
- Rust: rustfmt, clippy (treat warnings as errors).
- Python bindings (PyO3): build with maturin (abi3).

Local dev

```bash
rustup default stable
cargo fmt --all
cargo clippy -- -D warnings
cargo test

pip install maturin
maturin develop --release
python -c "import aif_core; print('import-ok')"
```

Security & compliance
- cargo-deny policy in deny.toml (advisories & licenses).
- GitHub Actions check fmt, clippy, tests, Python import smoke.
