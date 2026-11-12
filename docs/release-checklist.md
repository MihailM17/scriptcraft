# ScriptCraft release checklist

Use this checklist for every tagged release. A release is a build users keep;
never publish an artifact that has not been through the full gate below.

## 1. Scope and version

- [ ] `docs/v0.8-plan.md` milestones for the release are checked, or every
      remaining item is named in the release notes as deferred.
- [ ] `scriptcraft-app/Cargo.toml` version is bumped (the bundle script reads
      it). `Cargo.lock` is committed.
- [ ] `README.md` status/next section matches what is actually shipping.
- [ ] Release notes list user-visible changes and known limitations.
- [ ] No UI claim ships without a test or a verified manual run.

## 2. Gate (all must pass)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
git diff --check
```

Expected v0.8 counts (2026-11-10): app 52 interaction + 19 snapshots + 23 lib,
core 100 lib + 49 integration, layout 26 unit + 25 integration. Counts grow;
what matters is zero failures.

Run the benchmark smoke and compare against `docs/benchmarks.md`:

```sh
cargo bench -p scriptcraft-core --bench engine -- --quick
cargo bench -p scriptcraft-layout --bench layout -- --quick
```

## 3. macOS bundle

Build locally (this repo has no macOS CI signing secrets):

```sh
packaging/macos/bundle.sh
```

That produces `dist/ScriptCraft-<version>-macos-<arch>.dmg` and its `.sha256`.
Verify the mounted app:

```sh
hdiutil attach dist/ScriptCraft-<version>-macos-arm64.dmg
codesign --verify --deep --strict --verbose=2 \
  /Volumes/ScriptCraft\ <version>/ScriptCraft.app
# Only for a Developer ID build:
spctl --assess --type execute --verbose=2 \
  /Volumes/ScriptCraft\ <version>/ScriptCraft.app
hdiutil detach /Volumes/ScriptCraft\ <version>
shasum -a 256 -c dist/ScriptCraft-<version>-macos-arm64.dmg.sha256
```

### Developer ID and notarisation

Prerequisites (outside the repo; never commit credentials):

- A `Developer ID Application` certificate in the login keychain.
- `xcrun notarytool store-credentials scriptcraft-notary` with an Apple ID,
  team ID and app-specific password (or an App Store Connect API key).

Then run:

```sh
SIGN_IDENTITY="Developer ID Application: Name (TEAMID)" \
NOTARY_PROFILE="scriptcraft-notary" \
packaging/macos/bundle.sh
```

The script signs with hardened runtime (`--options runtime`), verifies,
submits the DMG, staples and validates it. Without both variables the build is
ad-hoc signed/not notarised and must be labelled that way; do not tell users it
is signed.

## 4. Windows

Windows code signing is **deferred** until a certificate path is chosen (Azure
Trusted Signing or an OV certificate). The zip produced by the `Packages`
workflow is unsigned. Release notes must say so and link the SmartScreen
instructions:

1. Download the zip and verify the published SHA-256.
2. SmartScreen → **More info** → **Run anyway**.
3. Do not disable SmartScreen globally.

If signing lands later, sign `ScriptCraft.exe` before zipping and verify with
`signtool verify /pa`.

## 5. Linux

The `Packages` workflow builds the tar.gz on Ubuntu 22.04. CI also runs the
interaction suite and a launch probe under Xvfb; the probe needs
`libxkbcommon-x11-dev` for winit. Release notes should name the GTK3
requirement.

## 6. Updates

There is no auto-updater in v0.8. Do not add a silent installer. Users install
a new DMG/zip over the old app; projects stay local. Update checking and signed
manifests move to v0.8.1 with their own tests.

## 7. Publish

- [ ] Tag `v<version>` at the gated commit and push the tag.
- [ ] Upload DMG, Windows zip, Linux tar.gz and all `.sha256` files.
- [ ] Re-download every asset and verify its checksum.
- [ ] Paste the release notes and known limitations.
- [ ] Update `ROADMAP.md` with the tag, commit, asset sizes and checksums.
