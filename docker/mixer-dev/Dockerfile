# Developer convenience layer on top of dev-tools-build.
#
# Adds what makes interactive work pleasant but is not required to build:
#   - shell tools (less, nvim, tmux, starship, direnv, kitty-terminfo)
#   - gpg + ssh client + sudo for ad-hoc work
#   - smartcard runtime helpers (pcscd, pcsc-tools, yubikey-manager,
#     pinentry-tty) for hands-on mpass YubiKey work
#   - the age-plugin-yubikey runtime binary mpass shells out to
#   - Claude Code CLI
#
# Anything actually needed to compile workspace crates or render the
# docs already lives in dev-tools-build.

FROM dev-tools-build:latest
ARG USER_NAME=dev
USER root

ARG DEV_PACKAGES="\
    less \
    neovim \
    tmux \
    starship \
    direnv \
    kitty-terminfo \
    iputils-ping \
    gnupg \
    openssh-client \
    sudo \
    pcscd \
    pcsc-tools \
    yubikey-manager \
    pinentry-tty \
"

RUN apt-get update && \
    apt-get install --no-install-recommends -y ${DEV_PACKAGES} && \
    rm -rf /var/lib/apt/lists/*

RUN set -eux; \
    usermod -aG sudo "${USER_NAME}"; \
    echo "${USER_NAME} ALL=(ALL) NOPASSWD:ALL" > "/etc/sudoers.d/${USER_NAME}"; \
    chmod 0440 "/etc/sudoers.d/${USER_NAME}"; \
    visudo -c

USER ${USER_NAME}

# Claude Code native binary (the npm install path is deprecated).
RUN curl -fsSL https://claude.ai/install.sh | bash

# age-plugin-yubikey: runtime tool that mpass shells out to at
# decrypt time for YubiKey-backed identities (ADR 0003 + the M3
# subsection in decisions/). The build deps (Rust toolchain,
# libpcsclite-dev, libusb-1.0-0-dev) come from dev-tools-build.
RUN cargo install --locked age-plugin-yubikey

ENV CLAUDE_CONFIG_DIR="/home/${USER_NAME}/.claude"

RUN mkdir -p \
    /home/${USER_NAME}/.config \
    /home/${USER_NAME}/.claude

COPY starship.toml   /home/${USER_NAME}/.config/
COPY bash_rc         /home/${USER_NAME}/.bashrc
COPY dot_gitconfig   /home/${USER_NAME}/.gitconfig
