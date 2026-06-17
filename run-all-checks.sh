#!/usr/bin/env bash
# Full local gate: formatting, lint, tests, docs, and supply-chain checks.
# Mirrors the CI workflow, so a green run here predicts a green run in CI.
# Runs every check even if an earlier one fails, then reports all failures.
# Exit code: 0 = all passed, 1 = a check failed, 2 = usage error.
set -euo pipefail
IFS=$'\n\t'

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT

declare -a FAILURES=()

usage() {
	cat >&2 <<'EOF'
Usage: run-all-checks.sh [-h]

Runs cargo fmt/clippy/test/doc across both feature sets, plus cargo-deny,
shellcheck, and shfmt when they are installed. Exit 0 if all pass, 1 if any
check fails, 2 on a usage error.
EOF
}

log() { printf '%s\n' "$*" >&2; }

# Run a named check. Records the failure instead of aborting, so one run
# surfaces every problem rather than only the first.
check() {
	local name="$1"
	shift
	log ""
	log "==> ${name}"
	if "$@"; then
		return 0
	fi
	log "FAILED: ${name}"
	FAILURES+=("${name}")
}

# Like check(), but skip with a note when the required tool is absent.
check_optional() {
	local name="$1" tool="$2"
	shift 2
	if ! command -v "${tool}" >/dev/null 2>&1; then
		log ""
		log "==> ${name} (skipped: ${tool} not installed)"
		return 0
	fi
	check "${name}" "$@"
}

main() {
	case "${1:-}" in
	-h | --help)
		usage
		exit 0
		;;
	"") ;;
	*)
		usage
		exit 2
		;;
	esac

	cd "${REPO_ROOT}"

	check "rustfmt" cargo fmt --all --check
	check "clippy (default features)" \
		cargo clippy --workspace --all-targets -- -D warnings
	check "clippy (no default features)" \
		cargo clippy --workspace --no-default-features --all-targets -- -D warnings
	check "tests (default features)" cargo test --workspace
	check "tests (no default features)" cargo test --workspace --no-default-features
	check "rustdoc" env RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
	check_optional "cargo-deny" cargo-deny cargo deny check
	check_optional "shellcheck" shellcheck shellcheck "${BASH_SOURCE[0]}"
	check_optional "shfmt" shfmt shfmt -d "${BASH_SOURCE[0]}"

	log ""
	if [[ ${#FAILURES[@]} -eq 0 ]]; then
		log "All checks passed."
		exit 0
	fi
	log "Failed checks:"
	local f
	for f in "${FAILURES[@]}"; do
		log "  - ${f}"
	done
	exit 1
}

main "$@"
