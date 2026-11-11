# Enstate Server (Docker / Binary)

## Development

Copy `.env.example` to `.env` and fill in the required environment variables. The ones that concern `.gwei` names
are described in the [README](../../README.md#configuration) at the root of the repository.

To run the caching and monitoring stack locally, you can run:

```sh
docker compose up -d
```

To test out your changes, you can run:

```sh
cargo run
```

### Tests

```sh
cargo test
```

The tests of the Redis cache layer need a Redis they can wipe and are skipped unless one is provided:

```sh
TEST_REDIS_URL=redis://127.0.0.1:6390 cargo test
```

### Metrics & Monitoring

The server exposes a `/metrics` endpoint that returns Prometheus metrics.
You can access the dashboard at [http://localhost:3000](http://localhost:3000).
