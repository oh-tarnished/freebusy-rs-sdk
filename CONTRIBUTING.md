# Contributing

This repository is generated: everything in it except `api/` is written by
[protoc-gen-sdk](https://github.com/the-protobuf-project/sdk) from the API's
protos and `api/sdk.yaml`, with Google's Rust GAPIC generator (librarian)
underneath. Changes to generated files are overwritten by the next
regeneration.

- To change the SDK's shape (names, services, docs, options), edit
  `api/sdk.yaml` and regenerate.
- To change hand-written helpers or examples, edit them under
  `api/custom-rust`, the source the generator copies them from.
- To change the API, change its protos; the regenerate workflow picks up the
  new release.

## Develop

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## Regenerate locally

```sh
brew install the-protobuf-project/tap/librarian  # librarian 0.53.0
cargo install taplo-cli                          # librarian formats Cargo.toml with it
go install github.com/the-protobuf-project/sdk/plugin/cmd/protoc-gen-sdk@v0.2.2
bash scripts/generate.sh
```

`scripts/generate.sh` runs `buf generate` in `api/`, then builds and tests
the workspace. No protoc is needed: protoc-gen-sdk answers librarian's
protoc calls from the descriptors buf compiled.

## Release

1. Add a `## [X.Y.Z]` section to `CHANGELOG.md` and merge it.
2. Tag `vX.Y.Z` and push the tag. The release workflow tests, sets the
   workspace's version to the tag's, attests the build provenance and SBOM
   of the packaged crates, publishes the GitHub release with the CHANGELOG
   section as notes, and publishes every crate to crates.io (the GAPIC
   crates first) by trusted publishing (OIDC; no token).

crates.io needs a trusted publisher for each crate once: this repository,
workflow `release.yaml`, environment `crates-io`.
