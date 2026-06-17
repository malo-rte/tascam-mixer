---
name: dev-container
description: Use whenever creating, editing, splitting, or reviewing Dockerfiles for a project's development container -- both the build image (everything needed to compile, test, and produce artifacts reproducibly; CI-safe) and the dev image (developer ergonomics layered on top via FROM). Encodes the build/dev contract, the package-selection rubric, host UID/GID matching, the runtime-vs-build dependency split, image-layering rules, and the enter-dev-container.sh companion script. Apply this any time a Dockerfile under docker/ is being authored or judged, not only when "dev container" is named explicitly.
---

# dev-container

How the project structures its development-container images and the
companion runner script. Stable IDs `DC-NN`. The cardinal rule is the
**build / dev contract**: a CI runner using the build image alone must
produce every artifact the project ships; the dev image adds only what
makes interactive work pleasant.

Examples in this skill are drawn from `dev-tools` (Rust workspace +
Wayland tools + smartcard / age-plugin-yubikey + AsciiDoc docs). The
rules generalise; substitute your project's specifics.

## The contract

Two images, layered. Both live under `docker/`.

| Image                 | Purpose                                                    | Who consumes it          |
|-----------------------|------------------------------------------------------------|--------------------------|
| `<project>-build`     | Compile, test, render docs. Reproducible. No human deps.   | CI, builders, dev image. |
| `<project>-dev`       | Build image + shell tools, editor, gpg, ssh, agent CLIs.   | Developers interactively. |

The dev image's first line is `FROM <project>-build:latest`. That
inheritance is non-negotiable: if you find yourself duplicating an apt
install across both, the dep belongs in build.

## DC-1 MUST: every build dependency lives in the build image

If `cargo build --workspace` or `make html pdf` would fail without a
package, it goes in `<project>-build`. This includes:

- Language toolchains (Rust, Go, Python interpreter, etc.).
- `-dev` header packages crates / extensions link against
  (`libwayland-dev`, `libpcsclite-dev`, `libusb-1.0-0-dev`,
  `libxkbcommon-dev`, ...).
- Document-build tools (`asciidoctor`, `ruby`, `default-jre-headless`
  for PlantUML, `graphviz`, the AsciiDoc gems).
- Scripting interpreters and modules the project's own tooling uses
  (`python3` + `python3-yaml` for the docs-librarian / tasklist
  scripts).
- Anything else CI must have to produce the artifact set.

Putting any of these in the dev image breaks the build/dev contract and
silently shifts work onto developer machines.

## DC-2 MUST: runtime dependencies of shipped artifacts also live in build

If a workspace binary shells out to or dynamically links against
something at run time, treat it as a build dep too: the build image is
the artifact-test surface, and a tested binary must be able to run.

Example: `mpass` shells out to `age-plugin-yubikey` at decrypt time.
The plugin binary is therefore a *runtime* dependency of `mpass`. It
sits in the dev image only because the project does not currently run
end-to-end mpass tests in CI; the moment such tests are added, the
plugin moves to build. Either keep it in dev *and* exclude
hardware-backed paths from CI, or move it to build *and* run them
hermetically. Don't accept "works in dev, fails in CI" as a steady
state.

## DC-3 SHOULD: developer-only tools live in dev

Anything no automated process needs:

- Shells and editors (`bash`, `neovim`, `tmux`).
- Prompt + ergonomics (`starship`, `direnv`, `less`, terminfo extras
  like `kitty-terminfo`).
- Networking / debugging convenience (`iputils-ping`).
- Personal-key tooling (`gnupg`, `openssh-client`).
- Privileged ad-hoc work (`sudo` + a NOPASSWD sudoers entry for the
  dev user).
- Smartcard runtime helpers when only humans drive them
  (`pcscd`, `pcsc-tools`, `yubikey-manager`, `pinentry-tty`).
- Agent CLIs (e.g. Claude Code via the upstream installer).

## DC-4 MUST: non-root user matches host UID/GID

Bind-mounted source files must come out owned by the host user, not
root. The build image creates the user; the dev image inherits and
adds sudo.

```dockerfile
# In the build image, after apt-get install:
ARG USER_NAME=dev
ARG USER_UID=1000
ARG USER_GID=1000

RUN groupadd -g ${USER_GID} ${USER_NAME} && \
    useradd -m -u ${USER_UID} -g ${USER_GID} -s /bin/bash ${USER_NAME}

USER ${USER_NAME}
```

The runner script (`enter-dev-container.sh`) passes `--build-arg
USER_UID="$(id -u)"` and `--build-arg USER_GID="$(id -g)"` so the
image is rebuilt per developer if their host UID changes.

In the dev image, switch back to root briefly to install packages and
configure sudo, then drop to the user:

```dockerfile
USER root
RUN apt-get update && ... && rm -rf /var/lib/apt/lists/*
RUN usermod -aG sudo "${USER_NAME}"; \
    echo "${USER_NAME} ALL=(ALL) NOPASSWD:ALL" > "/etc/sudoers.d/${USER_NAME}"
USER ${USER_NAME}
```

## DC-5 MUST: per-user installs go in the layer that owns them

Per-user installs (`cargo install`, `rustup`, `pipx`, `gem install
--user-install`) live in the same layer as the user that runs them.

- Rust toolchain via `rustup`: install in the **build** image as the
  `dev` user (since cargo is a build tool). The dev image inherits
  `/home/dev/.cargo/`.
- `age-plugin-yubikey` via `cargo install`: install in the **dev**
  image (it's a runtime tool; see DC-2). The Rust toolchain it needs
  to build itself is already in the inherited build image.
- Claude Code: dev image only.

System-wide installs (gems, npm globals, pipx) go in whichever image
needs them, before the `USER ${USER_NAME}` switch.

## DC-6 MUST: clean apt and gem caches in the same `RUN` as the install

```dockerfile
RUN apt-get update && \
    apt-get install --no-install-recommends -y ${PACKAGES} && \
    rm -rf /var/lib/apt/lists/*

RUN gem install --no-document <gems>
```

`--no-install-recommends` and `--no-document` avoid pulling in
documentation packages and gem doc archives that bloat layers
gigabytes for no developer value. The `rm -rf` in the same `RUN` keeps
the cache out of the layer entirely.

Never `apt-get clean` in a later layer -- it doesn't shrink earlier
layers in the squashed image.

## DC-7 SHOULD: order layers from least to most volatile

apt installs change rarely; `cargo install` of a single CLI changes
when the CLI's version moves; config files (`bash_rc`, `gitconfig`,
`starship.toml`) change often. Order Dockerfile statements so common
edits invalidate as few cached layers as possible:

1. `FROM`, `ARG`, `ENV` (project-stable).
2. apt installs.
3. gem / pipx / cargo system-wide installs.
4. User creation, sudoers (build image only).
5. Per-user tool installs (`rustup`, `cargo install`).
6. `COPY` of config files (dev image only).

Putting `COPY bash_rc /home/dev/.bashrc` at the top forces a full
rebuild on every shell-config tweak.

## DC-8 MUST NOT: carry stale project artifacts into the container context

Anything in the Dockerfile's build context that isn't a `COPY` source
or an `ADD` source is dead weight. Particularly:

- Generated build outputs (Yocto FIT images, target/ directories,
  Cargo `.lock` snapshots for crates already pinned).
- Old project's `.claude/settings.local.json` snapshots, dotfiles
  from a forked-from project, leftover `out/` trees.
- Anything carrying paths that no longer exist (`/workspaces/<old-project>/...`
  in a `PYTHONPATH`, `KAS_WORK_DIR` for a non-Yocto build).

Audit when forking from another project's `docker/` tree -- the
build/dev image rename is the easy part; deleting the dead apt
packages and ENV bindings is the work.

## The companion script (`enter-dev-container.sh`)

The runner script owns three things the Dockerfile cannot: build-arg
plumbing, host-resource pass-through, and image rebuild on demand.
Keep it small and POSIX-ish.

### DC-9 SHOULD: build both images, run only the dev image

```bash
build_container <image>   # docker build --build-arg USER_UID/GID -t <image>
build_container dev-tools-build
build_container dev-tools-dev   # FROMs dev-tools-build:latest
run_container   dev-tools-dev   # only the dev image gets `docker run`
```

CI invokes `build_container dev-tools-build` directly and runs `cargo
build` inside it via `docker run --rm dev-tools-build:latest cargo ...`.
The dev image is interactive-only.

### DC-10 MUST: bind-mount only what the work needs

- The repo, at `/workspaces/<repo-name>` (predictable, scriptable).
- The host's SSH agent socket and `~/.ssh` (read-only) so `git push`
  and remote operations work.
- USB bus when the project uses smartcards / hardware:
  `-v /dev/bus/usb:/dev/bus/usb` (bind-mount, not `--device`, so
  hotplug events propagate after `udevadm trigger` on the host).

What NOT to mount by default:

- Project-specific cache dirs (Yocto `downloads/`, `sstate-cache/`,
  SDK roots). They couple the container to one project's build model.
  Add them only if this project actually uses them.
- The host's `~/.config/<agent>/` unless persistence is required. The
  dev user's `$HOME` inside the container is usually enough.

### DC-11 SHOULD: honour SELinux mount labels

```bash
MOUNT_SUFFIX=""
if [[ -f /sys/fs/selinux/enforce ]] && [[ "$(cat /sys/fs/selinux/enforce)" != "0" ]]; then
    MOUNT_SUFFIX=":z"
fi
docker run ... -v "${REPO_DIR}:/workspaces/${REPO_NAME}${MOUNT_SUFFIX}" ...
```

`:z` relabels the mount so a Fedora / RHEL host with SELinux enforcing
can actually access it.

### DC-12 MUST: pass-through, never embed, host identity

Don't COPY the host's SSH keys, GPG keys, or git config into the
image. Bind-mount the agent socket and the relevant config dirs
read-only. An image that embeds a developer's secrets is a leak
waiting to happen.

## Gotchas worth listing

The lessons that round-trip on every project. These are not separate
rules; they are common DC-1..DC-12 violations.

- **Rust in the dev image breaks CI** (DC-1). If CI uses the build
  image and Rust lives in dev, `cargo build` fails in CI with
  cryptic "command not found". The build image owns toolchains.
- **USB hotplug needs bind-mount, not `--device`** (DC-10). With
  `--device /dev/bus/usb/001/002`, the dev container sees only the
  device that existed at `docker run` time; reconnecting the YubiKey
  inside the running shell silently fails. `-v /dev/bus/usb:/dev/bus/usb`
  plus `--group-add` for the right gid is what propagates hotplug.
- **PCSC needs the daemon, not just the libraries** (DC-3). Installing
  `libpcsclite1` is not enough for `ykman` or `age-plugin-yubikey` to
  work; the dev image must also run `pcscd` (typically via the package's
  default behaviour, or invoked from `bash_rc`).
- **A clone's apt list is the previous project's apt list** (DC-8).
  After `cp -r ../old-project/docker .`, every package in the
  Dockerfile is suspect until proven needed. The default position
  is to delete; add back what the build or runtime actually needs.
- **`--no-install-recommends` is not the default** (DC-6). Without
  it, every apt install pulls in suggested + recommended packages
  and the image balloons by a couple GB before you notice.

## Reviewing a Dockerfile change

Run through these on every diff that touches `docker/`:

1. Did anything that builds the project move to the dev image?
   Reject. (DC-1)
2. Did a developer-only tool move to the build image? Reject unless
   CI actually needs it. (DC-3)
3. Did the FROM line in the dev image change away from the
   `<project>-build:latest` inheritance? Major flag. (DC contract)
4. Is there an apt install without `--no-install-recommends`? (DC-6)
5. Is there an apt-get cache that survives into the squashed image?
   (DC-6)
6. Did a COPY of a config file move above an apt install? Layer
   cache fragility. (DC-7)
7. Are old paths to a fork-source project still in ARGs / ENVs / COPY
   sources? (DC-8)
8. Does the runner script pass `USER_UID` / `USER_GID`? (DC-4)

A diff that lands all eight checks doesn't need a build to review --
it'll behave.

## What this skill does not cover

- VS Code `.devcontainer.json` integration. The conventions here
  apply to the underlying Dockerfiles but the JSON wrapper is out of
  scope until the project adopts it.
- Multi-stage builds for runtime images of shipped binaries. This
  skill is about *developer* containers, not container images that
  ship to end users.
- Docker Compose / Kubernetes manifests for multi-service
  development. Out of scope until the workspace needs more than one
  container at once.
- Cargo / Ruby / Python registry mirror configuration. Add per-org
  if your project needs it; the skill stays vendor-neutral.
