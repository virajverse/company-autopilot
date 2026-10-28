# Windows Rust development setup

## Required tools

- Rust installed through `rustup`, using the stable `x86_64-pc-windows-msvc` toolchain.
- Cargo, rustfmt and Clippy (rustup default profile).
- Microsoft Visual Studio 2022 Build Tools with the **Desktop development with C++** workload, including MSVC and Windows SDK.
- Git for source control.

Python and `.venv` are not required for this Rust project. Cargo manages Rust dependencies; `Cargo.lock` records resolved versions and `target/` holds build output.

After installation, open a new terminal so the updated PATH takes effect. Rust executables normally reside in `%USERPROFILE%\.cargo\bin`.

## Verify installation

```powershell
rustup show
rustc --version
cargo --version
cargo fmt --version
cargo clippy --version
```

Version commands alone do not verify the MSVC linker/SDK. Compile and run a small Rust executable before starting application implementation.

## Application checks once the Cargo workspace exists

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The architecture's database, authentication, provider and deployment decisions remain in the roadmap. Install integration-specific tools when those choices are made; a Python environment is not part of the Rust foundation.
