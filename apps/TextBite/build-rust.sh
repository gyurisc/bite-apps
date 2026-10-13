#!/bin/sh
# Xcode build step for TextBite. Builds the Rust bridge (crates/bite-ffi) for
# the architectures Xcode is building and leaves one static library where the
# app target links it.
set -eu

PATH="$HOME/.cargo/bin:$PATH"
if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo not found. Install Rust from https://rustup.rs" >&2
    exit 1
fi

case "$CONFIGURATION" in
    Release) profile_dir=release; profile_flag=--release ;;
    *) profile_dir=debug; profile_flag= ;;
esac

workspace="$SRCROOT/../.."
set --
for arch in $ARCHS; do
    case "$arch" in
        arm64) target=aarch64-apple-darwin ;;
        x86_64) target=x86_64-apple-darwin ;;
        *) echo "error: no Rust target known for architecture $arch" >&2; exit 1 ;;
    esac
    cargo build --manifest-path "$workspace/Cargo.toml" -p bite-ffi --target "$target" $profile_flag
    set -- "$@" "$workspace/target/$target/$profile_dir/libbite_ffi.a"
done

# Replace the linked library only when it changed, so an unchanged engine does
# not force Xcode to relink the app.
library="$DERIVED_FILE_DIR/libbite_ffi.a"
mkdir -p "$DERIVED_FILE_DIR"
lipo -create "$@" -output "$library.new"
if cmp -s "$library.new" "$library"; then
    rm "$library.new"
else
    mv "$library.new" "$library"
fi
