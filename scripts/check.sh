#!/bin/sh
# Runs the checks AGENTS.md asks for, from any directory.
#
#   scripts/check.sh          Rust: formatting, Clippy, tests
#   scripts/check.sh --app    also builds the app and its UI tests
#   scripts/check.sh --ui     also runs the UI tests, which needs macOS
#                             Automation Mode (see README.md)
set -eu

cd "$(dirname "$0")/.."

case "$#:${1:-}" in
0: | 1:--app | 1:--ui) ;;
*)
	echo "usage: scripts/check.sh [--app | --ui]" >&2
	exit 2
	;;
esac

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Signed ad hoc, so no developer account is needed.
xcode() {
	xcodebuild "$1" \
		-project apps/TextBite/TextBite.xcodeproj \
		-scheme TextBite \
		-destination "platform=macOS,arch=$(uname -m)" \
		-quiet \
		CODE_SIGN_IDENTITY=- \
		DEVELOPMENT_TEAM=
}

case "${1:-}" in
--app) xcode build-for-testing ;;
--ui) xcode test ;;
esac
