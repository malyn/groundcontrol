# Just currently loads the `.env` file, but that will be changing, so we
# are disabling it now since we do not need the functionality.
set dotenv-load := false

_default:
    @just --list

# Generate and open the documentation for the crate
opendocs:
    cargo doc --open

# Display (non-dev) dependency tree
tree:
    cargo tree --edges normal

# Auto-build and run tests
watchtest:
    cargo watch -x "nextest run --all-features"