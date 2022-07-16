####################################################################################################
## Builder
####################################################################################################

FROM rust:1.60.0 AS builder

RUN apt-get update && apt-get install -y --no-install-recommends musl-tools musl-dev
RUN update-ca-certificates

WORKDIR /app
COPY ./ .
RUN cargo build --target x86_64-unknown-linux-musl --release


####################################################################################################
## Final image
####################################################################################################

FROM scratch

WORKDIR /
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/groundcontrol ./

ENTRYPOINT ["/groundcontrol"]