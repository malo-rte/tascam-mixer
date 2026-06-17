#!/usr/bin/env bash
# Render the project's AsciiDoc documents to PDF.
#
# Builds README.adoc and docs/*.adoc (or the files given as arguments) into
# build/docs/*.pdf using asciidoctor-pdf, with asciidoctor-diagram rendering the
# embedded PlantUML figures.
#
# Requirements: asciidoctor-pdf and asciidoctor-diagram (Ruby gems) and plantuml
# (with a JRE; Graphviz for some diagram types). On Debian/Ubuntu:
#   sudo gem install asciidoctor-pdf asciidoctor-diagram
#   sudo apt-get install plantuml graphviz
#
# Exit code: 0 if all documents built, 1 on a build or dependency error,
# 2 on a usage error.
set -euo pipefail
IFS=$'\n\t'

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly REPO_ROOT
OUT_DIR="${REPO_ROOT}/build/docs"
readonly OUT_DIR

usage() {
	cat >&2 <<'EOF'
Usage: docs/build.sh [-h] [file.adoc ...]

Render AsciiDoc documents to PDF under build/docs/. With no arguments, builds
README.adoc and docs/*.adoc. Requires asciidoctor-pdf, asciidoctor-diagram, and
plantuml.
EOF
}

log() { printf '%s\n' "$*" >&2; }

require() {
	local tool="$1"
	if ! command -v "${tool}" >/dev/null 2>&1; then
		log "error: required tool '${tool}' not found; see the header of $0 for install steps"
		exit 1
	fi
}

main() {
	case "${1:-}" in
	-h | --help)
		usage
		exit 0
		;;
	esac

	require asciidoctor-pdf
	require plantuml

	cd "${REPO_ROOT}"

	local -a docs
	if [[ $# -gt 0 ]]; then
		docs=("$@")
	else
		docs=(README.adoc)
		local f
		for f in docs/*.adoc; do
			docs+=("${f}")
		done
	fi

	mkdir -p "${OUT_DIR}"

	local doc rc=0
	for doc in "${docs[@]}"; do
		if [[ ! -f "${doc}" ]]; then
			log "error: no such document: ${doc}"
			rc=1
			continue
		fi
		log "==> ${doc}"
		if asciidoctor-pdf -r asciidoctor-diagram -D "${OUT_DIR}" "${doc}"; then
			log "    -> ${OUT_DIR}/$(basename "${doc%.adoc}").pdf"
		else
			log "    FAILED: ${doc}"
			rc=1
		fi
	done

	return "${rc}"
}

main "$@"
