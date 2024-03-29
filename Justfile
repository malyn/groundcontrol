# Just currently loads the `.env` file, but that will be changing, so we
# are disabling it now since we do not need the functionality.
set dotenv-load := false

_default:
    @just --list

# Generate and open the documentation for the crate
opendocs:
    cargo doc --open

# Check licenses, unmaintained crates, vulnerabilities, etc.
check:
    cargo deny check

# Display (non-dev) dependency tree
tree:
    cargo tree --edges normal

# Auto-build and run tests
watchtest:
    cargo watch -x "nextest run --all-features --no-fail-fast"

# Preflight checklist. Does everything that could fail on the build machine
preflight:
    cargo fmt --all -- --check
    cargo clippy --all --all-features -- --deny warnings
    cargo deny check
    cargo nextest run --all-features