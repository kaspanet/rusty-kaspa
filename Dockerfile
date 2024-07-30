FROM rust:1.80 AS builder

# ldd links against libstdc++ otherwise and it's not in distroless.
ENV RUSTFLAGS='-C target-feature=+crt-static'
WORKDIR /kaspa
# remove the docker-clean file that prevents apt from caching.
RUN rm -f /etc/apt/apt.conf.d/docker-clean
# install dependencies
RUN --mount=target=/var/lib/apt/lists,type=cache,sharing=locked \
    --mount=target=/var/cache/apt,type=cache,sharing=locked \
    apt-get update \
    && apt-get install -y --no-install-recommends \
    build-essential \
    protobuf-compiler \
    libclang-dev 

# copy the source code
COPY . . 

# build the binary. only kaspad is needed.
RUN --mount=target=/kaspa/target,type=cache,sharing=locked \
    # allow for aarch64 builds
    export ARCH=$(uname -m); \
    # target is needed since RUSTFLAGS are set. see: https://github.com/rust-lang/rust/issues/78210
    cargo build --release --bin kaspad --target ${ARCH}-unknown-linux-gnu \
    && mv target/${ARCH}-unknown-linux-gnu/release/kaspad /kaspad

# build the final image
FROM gcr.io/distroless/static-debian12
COPY --from=builder /kaspad /
# the localhost default doesn't work containerized. you still need to deal with container NAT to expose this to a potential client.
ENV KASPAD_RPCLISTEN="0.0.0.0:16110"
ENV KASPAD_RPCLISTEN_BORSH="0.0.0.0:17110"
ENV KASPAD_RPCLISTEN_JSON="0.0.0.0:18110"

ENTRYPOINT [ "/kaspad" ]
