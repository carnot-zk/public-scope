FROM rust:bookworm AS rust-builder
RUN apt-get update && apt-get install -y \
  pkg-config libssl-dev clang lld curl git \
  protobuf-compiler build-essential \
  && rm -rf /var/lib/apt/lists/*
RUN curl -L https://sp1.succinct.xyz | bash && /root/.sp1/bin/sp1up
ENV PATH="/root/.sp1/bin:${PATH}"
WORKDIR /build
COPY circuit /build/circuit
RUN cargo build --manifest-path /build/circuit/script/Cargo.toml --bin carnot --release

FROM node:22-bookworm AS node-builder
WORKDIR /workspace
COPY sdk/package.json sdk/package-lock.json ./sdk/
WORKDIR /workspace/sdk
RUN npm ci
COPY sdk/ ./
RUN npm run build

WORKDIR /workspace
COPY keeper/package.json keeper/package-lock.json ./keeper/
WORKDIR /workspace/keeper
RUN npm ci
COPY keeper/ ./
RUN npm run build && npm prune --omit=dev

FROM node:22-bookworm AS runner
RUN apt-get update && apt-get install -y ca-certificates \
  && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=rust-builder /build/circuit/target/release/carnot /prover/carnot
RUN chmod +x /prover/carnot
COPY --from=node-builder /workspace/sdk /app/sdk
COPY --from=node-builder /workspace/keeper /app/keeper
WORKDIR /app/keeper
ENV NODE_ENV=production
ENV SP1_PROVER_BINARY=/prover/carnot
CMD ["node", "dist/index.js"]
