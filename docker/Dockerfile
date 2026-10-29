# syntax=docker/dockerfile:1

# Extra CAs for TLS-intercepting proxies. Empty unless overridden with
# --build-context extra-ca=<dir of .crt files>.
FROM scratch AS extra-ca

FROM rust:1.99.0-slim-trixie AS build
COPY --from=extra-ca . /usr/local/share/ca-certificates/
RUN update-ca-certificates
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked && cp target/release/vimify /vimify

FROM debian:trixie-slim
ARG TARGETARCH
ARG LAZYGIT_VERSION=0.66.1

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl git less \
    && rm -rf /var/lib/apt/lists/*
COPY --from=extra-ca . /usr/local/share/ca-certificates/
RUN update-ca-certificates

RUN set -eu; \
    case "$TARGETARCH" in \
      amd64) arch=x86_64 ;; \
      arm64) arch=arm64 ;; \
      *) echo "unsupported arch: $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    install_release() { \
      repo=$1 tag=$2 file=$3 bin=$4; \
      base="https://github.com/$repo/releases/download/$tag"; \
      curl -fsSLO "$base/$file"; \
      curl -fsSL "$base/checksums.txt" | grep " $file\$" | sha256sum -c -; \
      tar -xzf "$file" -C /usr/local/bin "$bin"; \
      rm "$file"; \
    }; \
    cd /tmp; \
    install_release jesseduffield/lazygit "v$LAZYGIT_VERSION" "lazygit_${LAZYGIT_VERSION}_linux_${arch}.tar.gz" lazygit

COPY --from=build /vimify /usr/local/bin/vimify

RUN useradd -m -s /bin/bash dev
USER dev
WORKDIR /home/dev
ENV TERM=xterm-256color

# A repo so lazygit has something to show.
RUN git config --global user.name dev \
    && git config --global user.email dev@localhost \
    && git init -q demo \
    && git -C demo commit -q --allow-empty -m init

CMD ["bash"]
