FROM rust:buster AS build

COPY src /build/src
COPY Cargo.toml /build/
WORKDIR /build

RUN rustup toolchain install nightly-aarch64-unknown-linux-gnu
RUN cargo +nightly -Zgitoxide build --release

# FIXME: Replace this with musl build to scratch image
FROM debian:buster-slim AS deploy

COPY --from=build /build/target/release/azos-keeper /azos-keeper

CMD ["/azos-keeper"]