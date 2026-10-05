FROM rust:1-slim AS rust-build
WORKDIR /src
COPY Cargo.* ./
COPY crates crates
RUN cargo build --release -p prism-cli

FROM node:20-slim AS web-build
WORKDIR /web
COPY web/package*.json ./
RUN npm ci
COPY web .
RUN npm run build

FROM debian:bookworm-slim
COPY --from=rust-build /src/target/release/prism /usr/local/bin/prism
COPY --from=web-build /web/dist /usr/share/prism/web
ENTRYPOINT ["prism"]
CMD ["info"]
