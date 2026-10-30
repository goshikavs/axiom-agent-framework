# Axiom Studio. Build from the repository root:
#   docker build -f apps/studio/Dockerfile -t axiom-studio .
# Bind 0.0.0.0 inside the container; map 127.0.0.1:3000 on the host.

FROM oven/bun:1.2-debian AS deps
WORKDIR /app
COPY apps/studio/package.json ./
RUN bun install

FROM oven/bun:1.2-debian AS build
WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY apps/studio ./
ENV NEXT_TELEMETRY_DISABLED=1
RUN bun run build

FROM oven/bun:1.2-debian
WORKDIR /app
ENV NODE_ENV=production
ENV NEXT_TELEMETRY_DISABLED=1
COPY --from=build /app ./
EXPOSE 3000
USER bun
CMD ["bun", "x", "next", "start", "--port", "3000", "--hostname", "0.0.0.0"]
