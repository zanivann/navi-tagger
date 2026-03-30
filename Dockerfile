# Estágio de Compilação
FROM rust:1.85-slim AS builder

WORKDIR /app
COPY . .

# Instala dependências de compilação
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Compila a release
RUN cargo build --release

# Estágio de Execução
FROM debian:bookworm-slim

WORKDIR /app

# Instala dependências mínimas de runtime
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

# Copia o binário do builder
COPY --from=builder /app/target/release/navi-tagger .

# Porta do servidor
EXPOSE 3000

# Execução
CMD ["./navi-tagger"]