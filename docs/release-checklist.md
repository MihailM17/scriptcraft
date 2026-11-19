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

Expected v0.8.11 counts (2026-11-19): app 89 lib + 81 interaction + 35
snapshots, core 113 lib + 50 integration, layout 30 lib + 27 integration.
Counts grow; what matters is zero failures.

Timing gates use best-of-five samples and a CI-tolerant unit threshold; the
Criterion benches in `docs/benchmarks.md` remain the real performance budget.

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

Since v0.8.8 the app can install a detected release itself: it downloads the
platform asset and its `.sha256` into `updates/<version>/` in the per-user
config folder, verifies the digest, and starts a helper that waits for this
process to exit before replacing the app and relaunching. It is **not** a
silent updater: the prompt always asks, and Cancel/Later are real actions.

Release requirements:

- [ ] Every platform archive is uploaded with a matching `<asset>.sha256`
      (`shasum -a 256` / `sha256sum` format: `<hex>  <name>`). Selection and
      verification are covered by `scriptcraft-app/src/update.rs` tests.
- [ ] Asset names keep the `<os>-<arch>` tags the selector expects
      (`macos-arm64`, `windows-x64`, `linux-x86_64`); an OS-only name is the
      fallback.
- [ ] Re-run `helper_script_is_valid_shell_syntax`; it parses the generated
      POSIX helper with `sh -n`. The Windows PowerShell helper is exercised
      only by inspection on a non-Windows host.
- [ ] Release notes say the checksum verifies the transfer, not the publisher,
      and that unsigned builds still trigger Gatekeeper/SmartScreen.
- [ ] An install from a read-only location (mounted DMG) must fail with the
      "move it to Applications" message, not a half-replaced app.

Do not describe this as signed, notarised or silent distribution. Signed
manifests and code signing remain deferred.

## 7. Publish

- [ ] Tag `v<version>` at the gated commit and push the tag.
- [ ] Upload DMG, Windows zip, Linux tar.gz and all `.sha256` files.
- [ ] Re-download every asset and verify its checksum.
- [ ] Paste the release notes and known limitations.
- [ ] Update `ROADMAP.md` with the tag, commit, asset sizes and checksums.
