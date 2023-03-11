# syntax=docker/dockerfile:1

########################################################################
## xx Cross-Compilation Helper
########################################################################

FROM --platform=${BUILDPLATFORM} tonistiigi/xx:1.2.1 AS xx


########################################################################
## Rust Builder
####################################################################################################

FROM --platform=${BUILDPLATFORM} rust:1.68.0-alpine3.17 as builder

# Copy over the xx cross-compilation helpers, then install the required
# compilers, linkers and development libraries.
COPY --from=xx / /
RUN apk update && apk add --no-cache clang lld musl-dev

# Copy the source itself.
WORKDIR /app
COPY ./ .

# Build the Rust binary (for the target platform).
ARG TARGETPLATFORM
RUN CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse \
    xx-cargo build --release --target-dir ./build && \
    xx-verify ./build/$(xx-cargo --print-target-triple)/release/groundcontrol && \
    cp ./build/$(xx-cargo --print-target-triple)/release/groundcontrol /groundcontrol


########################################################################
## Final Image
########################################################################

FROM scratch

COPY --from=builder /groundcontrol /groundcontrol

ENTRYPOINT ["/groundcontrol"]