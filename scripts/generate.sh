#!/usr/bin/env bash
# Regenerates this repository from api/: builds the reference images
# api/sdk.yaml lists, runs protoc-gen-sdk over the API with buf, then
# builds and tests the workspace.
#
# Needs buf, protoc-gen-sdk, librarian, cargo and taplo on PATH
# (CONTRIBUTING.md).
set -euo pipefail
cd "$(dirname "$0")/../api"
buf build buf.build/the-protobuf-project/rfc:3aba0422074b4341aa0f8df07c3263b7 -o rfc.binpb
rm -rf ../gapic
buf generate
cd ..
# SDK_SKIP_TESTS=1 stops after generating (the GitHub Action's test: false).
if [ -n "${SDK_SKIP_TESTS:-}" ]; then exit 0; fi
cargo fmt --all --check
cargo test --workspace
cargo clippy -p shokki-freebusy --all-targets -- -D warnings
