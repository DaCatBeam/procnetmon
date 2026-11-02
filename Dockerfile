FROM rust:1.99-bookworm

WORKDIR /app
COPY . .
ENV RUST_MIN_STACK=33554432
RUN cargo build

CMD ["/app/target/debug/procnetmon", "--poll-interval", "1000"]
