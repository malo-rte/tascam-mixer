#!/usr/bin/env  bash
set -euo pipefail

BUILD_IMAGE="mixer-build"
DEV_IMAGE="mixer-dev"

MOUNT_SUFFIX=""
if [[ -f /sys/fs/selinux/enforce ]] && [[ "$(cat /sys/fs/selinux/enforce)" != "0" ]]; then
	MOUNT_SUFFIX=":z"
fi

REPO_DIR="$(git rev-parse --show-toplevel 2>/dev/null)"
REPO_NAME="$(basename "${REPO_DIR}")"

REPO_DIR_CONTAINER="${REPO_DIR_CONTAINER:-/workspaces/${REPO_NAME}}"

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"

CLAUDE_STATE_HOST="${XDG_STATE_HOME:-$HOME/.local/state}/claude-dev/${REPO_NAME}"
CLAUDE_CONFIG_CONTAINER="/home/$(id -un)/.claude"

mkdir -p "${CLAUDE_STATE_HOST}"

CLAUDE_FLAGS=(-v "${CLAUDE_STATE_HOST}:${CLAUDE_CONFIG_CONTAINER}${MOUNT_SUFFIX}")
RUST_FLAGS=(-v "claude-dev-cargo:/home/$(id -un)/.cargo/registry${MOUNT_SUFFIX}")

SSH_FLAGS=()
if [[ -n "${SSH_AUTH_SOCK:-}" && -S "${SSH_AUTH_SOCK}" ]]; then
	SSH_FLAGS+=(-v "${SSH_AUTH_SOCK}:${SSH_AUTH_SOCK}" -e SSH_AUTH_SOCK)
	# Add the agent's group to avoid EACCES on the socket
	if command -v stat >/dev/null 2>&1; then
		SSH_GID="$(stat -c %g "$SSH_AUTH_SOCK" 2>/dev/null || echo "")"
		[[ -n "$SSH_GID" ]] && SSH_FLAGS+=(--group-add "$SSH_GID")
	fi
fi

# Mount user SSH config read-only into the container
if [[ -d "${HOME}/.ssh" ]]; then
	SSH_FLAGS+=(-v "${HOME}/.ssh:/home/$(id -un)/.ssh:ro${MOUNT_SUFFIX}")
fi

# USB pass-through (for YubiKey / smartcard work via age-plugin-yubikey).
# Bind-mounting /dev/bus/usb (rather than --device) propagates hotplug
# events from the host: a YubiKey unplugged + replugged after the
# container starts becomes visible inside. The bind-mount alone only
# makes the device nodes visible — Docker's default device cgroup still
# denies I/O on them. --device-cgroup-rule 'c 189:* rwm' grants r/w/mknod
# on USB character devices (major 189), so libusb_open works for any
# device that appears later via hotplug.
USB_FLAGS=()
if [[ -d /dev/bus/usb ]]; then
	USB_FLAGS+=(
		--volume "/dev/bus/usb:/dev/bus/usb${MOUNT_SUFFIX}"
		--device-cgroup-rule "c 189:* rwm"
	)
	# Map the host's plugdev group if it exists (Debian/Ubuntu YubiKey
	# udev rules typically grant `0664 root:plugdev` on the device file).
	if command -v getent >/dev/null 2>&1; then
		PLUGDEV_GID="$(getent group plugdev 2>/dev/null | cut -d: -f3 || true)"
		[[ -n "${PLUGDEV_GID}" ]] && USB_FLAGS+=(--group-add "${PLUGDEV_GID}")
	fi
else
	echo "Note: /dev/bus/usb not present on host. YubiKey work will not be possible." >&2
fi

# ALSA sound-device pass-through (MIDI to the US-16x08 / GX-700, and audio cards).
# Mirrors the USB block: bind-mounting /dev/snd propagates hotplug, so an
# interface plugged in after the container starts becomes visible; the cgroup
# rule grants r/w on ALSA character devices (major 116), which Docker's default
# device cgroup otherwise denies.
SND_FLAGS=()
if [[ -d /dev/snd ]]; then
	SND_FLAGS+=(
		--volume "/dev/snd:/dev/snd${MOUNT_SUFFIX}"
		--device-cgroup-rule "c 116:* rwm"
	)
	# /dev/snd nodes are group-owned by `audio` (mode 0660); add that gid so
	# opening them does not hit EACCES.
	if command -v getent >/dev/null 2>&1; then
		AUDIO_GID="$(getent group audio 2>/dev/null | cut -d: -f3 || true)"
		[[ -n "${AUDIO_GID}" ]] && SND_FLAGS+=(--group-add "${AUDIO_GID}")
	fi
else
	echo "Note: /dev/snd not present on host. MIDI/audio device work will not be possible." >&2
fi

build_container() {
	local image="$1"

	docker build \
		--build-arg USER_NAME="$(id -un)" \
		--build-arg USER_UID="$(id -u)" \
		--build-arg USER_GID="$(id -g)" \
		--build-arg REPO_DIR="${REPO_DIR_CONTAINER}" \
		-f "${SCRIPT_DIR}/${image}/Dockerfile" \
		-t "${image}" \
		"${SCRIPT_DIR}/${image}"
}

run_container() {
	local image="$1"
	local tz_flags=()
	local locale_flags=()

	if [[ -f /etc/localtime ]]; then
		tz_flags+=(--volume "/etc/localtime:/etc/localtime:ro${MOUNT_SUFFIX}")
	fi

	if [[ -f /etc/timezone ]]; then
		local host_tz
		host_tz="$(tr -d '\n' </etc/timezone)"
		tz_flags+=(--volume "/etc/timezone:/etc/timezone:ro${MOUNT_SUFFIX}")
		[[ -n "${host_tz}" ]] && tz_flags+=(-e "TZ=${host_tz}")
	fi

	[[ -n "${LANG:-}" ]] && locale_flags+=(-e "LANG=${LANG}")
	[[ -n "${LC_ALL:-}" ]] && locale_flags+=(-e "LC_ALL=${LC_ALL}")
	[[ -n "${LC_TIME:-}" ]] && locale_flags+=(-e "LC_TIME=${LC_TIME}")

	docker run --rm -it --init \
		--user "$(id -u):$(id -g)" \
		"${CLAUDE_FLAGS[@]}" \
		"${RUST_FLAGS[@]}" \
		"${SSH_FLAGS[@]}" \
		"${USB_FLAGS[@]}" \
		"${SND_FLAGS[@]}" \
		"${tz_flags[@]}" \
		"${locale_flags[@]}" \
		--volume "${REPO_DIR}:${REPO_DIR_CONTAINER}${MOUNT_SUFFIX}" \
		-w "${REPO_DIR_CONTAINER}" \
		"${image}:latest" bash
}

build_container "${BUILD_IMAGE}"
build_container "${DEV_IMAGE}"

run_container "${DEV_IMAGE}"
