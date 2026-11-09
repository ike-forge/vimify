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

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       bat btop ca-certificates curl git less python3 sqlite3 tmux unzip \
    && rm -rf /var/lib/apt/lists/*
# Debian ships bat as batcat.
RUN ln -s /usr/bin/batcat /usr/local/bin/bat
COPY --from=extra-ca . /usr/local/share/ca-certificates/
RUN update-ca-certificates

COPY docker/tools.lock /tmp/tools.lock
RUN set -eu; \
    cd /tmp; \
    while read -r arch sha url bins; do \
      [ "$arch" = "$TARGETARCH" ] || continue; \
      file=${url##*/}; \
      curl -fsSLo "$file" "$url"; \
      echo "$sha  $file" | sha256sum -c -; \
      mkdir x; \
      case "$file" in \
        *.zip) unzip -q "$file" -d x ;; \
        *) tar -xzf "$file" -C x ;; \
      esac; \
      for bin in $bins; do \
        find x -type f -name "$bin" -exec install -m 755 {} /usr/local/bin/ \; ; \
        [ -x "/usr/local/bin/$bin" ] || { echo "$bin not in $file" >&2; exit 1; }; \
      done; \
      rm -rf x "$file"; \
    done < tools.lock; \
    rm tools.lock

COPY --from=build /vimify /usr/local/bin/vimify

COPY docker/entrypoint.sh /usr/local/bin/sandbox-entrypoint
COPY docker/e2e.sh /usr/local/bin/sandbox-e2e
RUN useradd -m -s /bin/bash dev
USER dev
WORKDIR /home/dev
ENV TERM=xterm-256color COLORTERM=truecolor

COPY --chown=dev docker/seed.sh /tmp/seed.sh
RUN sh /tmp/seed.sh && rm /tmp/seed.sh

ENTRYPOINT ["sandbox-entrypoint"]
CMD ["bash"]
