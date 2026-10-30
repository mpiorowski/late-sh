ARG RUST_VERSION=1.97
ARG DEBIAN_VERSION=bookworm

FROM --platform=$TARGETPLATFORM rust:${RUST_VERSION}-slim-${DEBIAN_VERSION}

RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake \
    make \
    pkg-config \
    libssl-dev \
    perl \
    clang \
    mold \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN echo '[target.x86_64-unknown-linux-gnu]\nlinker = "clang"\nrustflags = ["-C", "link-arg=-fuse-ld=mold"]\n\n[target.aarch64-unknown-linux-gnu]\nlinker = "clang"\nrustflags = ["-C", "link-arg=-fuse-ld=mold"]' >> /usr/local/cargo/config.toml

RUN cargo install cargo-watch --locked

WORKDIR /app
ENV CARGO_TARGET_DIR=/app/target

CMD ["cargo", "watch", "-w", "late-ssh", "-w", "late-core", "-w", "assets", "-w", "Cargo.toml", "-w", "Cargo.lock", "-x", "run -p late-ssh"]
