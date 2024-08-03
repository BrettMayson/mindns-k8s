FROM rust:alpine3.20 as builder
WORKDIR /usr/src/app
COPY . .
RUN apk add --no-cache musl-dev
RUN cargo build --release

FROM alpine:3.20
RUN apk add --no-cache libgcc openssh
COPY --from=builder /usr/src/app/target/release/mindns-k8s /usr/local/bin/mindns-k8s
RUN mkdir /app
WORKDIR /app
CMD ["mindns-k8s"]
