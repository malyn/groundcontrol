####################################################################################################
## Builder
####################################################################################################

FROM rust:1.62.0-alpine3.16 AS builder

RUN apk update && apk add --no-cache musl-dev

WORKDIR /app
COPY ./ .
RUN cargo build --release


####################################################################################################
## Final image
####################################################################################################

FROM scratch

WORKDIR /
COPY --from=builder /app/target/release/groundcontrol ./

ENTRYPOINT ["/groundcontrol"]