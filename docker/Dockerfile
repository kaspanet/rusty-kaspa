FROM rust:1.79.0-bookworm AS builder

RUN apt-get update
RUN apt-get install -y build-essential libssl-dev pkg-config
RUN apt-get install -y protobuf-compiler libprotobuf-dev
RUN apt-get install -y clang-format clang-tidy \
        clang-tools clang clangd libc++-dev \
        libc++1 libc++abi-dev libc++abi1 \
        libclang-dev libclang1 liblldb-dev \
        libllvm-ocaml-dev libomp-dev libomp5 \
        lld lldb llvm-dev llvm-runtime \
        llvm python3-clang 

WORKDIR /app

COPY . .

RUN rustup target add x86_64-unknown-linux-gnu

RUN cargo build --release --target x86_64-unknown-linux-gnu --bin kaspad --bin kaspa-wallet

FROM debian:bookworm

RUN apt-get update

# fix Reqwest error: unable to get local issuer certificate
RUN apt-get install -y ca-certificates

WORKDIR app

COPY --from=builder /app/target/x86_64-unknown-linux-gnu/release/kaspad .
COPY --from=builder /app/target/x86_64-unknown-linux-gnu/release/kaspa-wallet .

CMD ["./kaspad"]
