FROM rust:bookworm AS builder

RUN apt-get update
RUN apt-get install -y curl git build-essential libssl-dev pkg-config 
RUN apt-get install -y protobuf-compiler libprotobuf-dev
RUN apt-get install -y clang-format clang-tidy \
        clang-tools clang clangd libc++-dev \
        libc++1 libc++abi-dev libc++abi1 \
        libclang-dev libclang1 liblldb-dev \
        libllvm-ocaml-dev libomp-dev libomp5 \
        lld lldb llvm-dev llvm-runtime \
        llvm python3-clang

RUN cargo install wasm-pack
RUN rustup target add wasm32-unknown-unknown

COPY . /rusty-kaspa
WORKDIR /rusty-kaspa
RUN cargo build --release


FROM debian:bookworm

COPY --from=builder /rusty-kaspa/target/release/kaspad /usr/bin/
COPY --from=builder /rusty-kaspa/target/release/kaspa-wallet /usr/bin/
COPY --from=builder /rusty-kaspa/target/release/kaspa_p2p_client /usr/bin/
COPY --from=builder /rusty-kaspa/target/release/kaspa_p2p_server /usr/bin/
COPY --from=builder /rusty-kaspa/target/release/kaspa_p2p_server /usr/bin/
COPY --from=builder /rusty-kaspa/target/release/kaspa-wrpc-proxy /usr/bin/
COPY --from=builder /rusty-kaspa/target/release/simpa /usr/bin/
COPY --from=builder /rusty-kaspa/target/release/rothschild /usr/bin/
