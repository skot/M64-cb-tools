#!/bin/sh
# Build the tools independently of Mujina and package a RAM-deployable bundle.
set -eu
TOOLS_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
REPO_DIR=$TOOLS_DIR
TARGET=aarch64-unknown-linux-musl
OUT="$REPO_DIR/target/m64-cb-tools"
BUNDLE="$OUT/m64-cb-tools-$TARGET"
mkdir -p "$BUNDLE"
cargo build --manifest-path "$REPO_DIR/Cargo.toml" -p m64-cb-tools \
    --locked --release --target "$TARGET" --target-dir "$REPO_DIR/target" \
    --config 'target.aarch64-unknown-linux-musl.linker="rust-lld"'
for binary in m64-board-info controlboard-misc fan-tool p738b-psu-tool hashboard-m64 i2c-probe; do
    cp "$REPO_DIR/target/$TARGET/release/$binary" "$BUNDLE/$binary"
done
cp "$TOOLS_DIR/scripts/capture-status.sh" "$BUNDLE/capture-status.sh"
cp "$TOOLS_DIR/README.md" "$BUNDLE/README.md"
cp "$TOOLS_DIR/INTERFACES.md" "$BUNDLE/INTERFACES.md"
cp "$REPO_DIR/LICENSE" "$BUNDLE/LICENSE"
(
    cd "$BUNDLE"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum m64-board-info controlboard-misc fan-tool p738b-psu-tool hashboard-m64 i2c-probe capture-status.sh > SHA256SUMS
    else
        shasum -a 256 m64-board-info controlboard-misc fan-tool p738b-psu-tool hashboard-m64 i2c-probe capture-status.sh > SHA256SUMS
    fi
)
tar -czf "$OUT/m64-cb-tools-$TARGET.tar.gz" -C "$OUT" "m64-cb-tools-$TARGET"
printf 'Bundle: %s\n' "$OUT/m64-cb-tools-$TARGET.tar.gz"
