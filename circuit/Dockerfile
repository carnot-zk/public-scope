FROM rust:bookworm AS builder
RUN apt-get update && apt-get install -y \
  pkg-config libssl-dev clang lld curl git \
  protobuf-compiler build-essential \
  && rm -rf /var/lib/apt/lists/*
RUN curl -L https://sp1.succinct.xyz | bash && /root/.sp1/bin/sp1up
ENV PATH="/root/.sp1/bin:${PATH}"
WORKDIR /build
COPY . .
RUN cargo build --manifest-path /build/script/Cargo.toml --bin carnot --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/carnot /usr/local/bin/carnot
ENTRYPOINT ["carnot"]
