FROM rust:1.99.1-alpine AS build
RUN apk add --no-cache musl-dev python3
WORKDIR /app
COPY . .
RUN cargo build --release

FROM scratch
COPY --from=build /app/target/release/cssdiff /cssdiff
ENTRYPOINT ["/cssdiff"]
