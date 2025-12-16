# Elevation service

Self-hosted elevation service that works with the [terrain data provided by Mapzen and Amazon AWS S3](https://registry.opendata.aws/terrain-tiles/). You can either pre-download the entire data on your server (ca. 200 GB), access public S3 buckets directly over HTTP/HTTPS, or use authenticated S3 access with credentials for private buckets and S3-compatible services like MinIO or DigitalOcean Spaces.

Try it out with our hosted service: https://elevation.racemap.com/api

Inspired by:

- https://github.com/perliedman/elevation-service
- https://github.com/perliedman/node-hgt

## API usage

The service has a very simple API. Just post your latitude-longitude pairs as a JSON array to the service and receive an array of elevations as response. Maximum post payload is by default 700 KB (which fits roughly 10,000 points).

```bash
# > [[lat, lng], ...]
curl -d '[[51.3, 13.4], [51.4, 13.3]]' -XPOST -H 'Content-Type: application/json' http://localhost:3000
# < [ele, ...]
```

For one-off queries. You can also issue GET requests with latitude and longitude as query parameters.

```bash
curl 'http://localhost:3000/?lat=51.3&lng=13.4'
# < ele
```

## Resource Management

The elevation service includes several configuration options to control resource usage and limit concurrency:

### Limiting Tokio Runtime Threads

To limit the number of threads used by the tokio runtime (useful for containerized environments). Should be not greater, then the number of cpu cores:

```bash
docker run --rm \
  -eTILE_SET_PATH=s3://elevation-tiles-prod/skadi \
  -eMAX_TOKIO_THREADS=4 \
  -p3000:3000 racemap/elevation-service
```

### Limiting Concurrent Request Handlers

To limit the number of concurrent request handlers (helps prevent overload):

```bash
docker run --rm \
  -eTILE_SET_PATH=s3://elevation-tiles-prod/skadi \
  -eMAX_CONCURRENT_HANDLERS=100 \
  -p3000:3000 racemap/elevation-service
```

### Limiting Parallel Processing in Batch Requests

To control the number of parallel elevation lookups within batch requests:

```bash
docker run --rm \
  -eTILE_SET_PATH=s3://elevation-tiles-prod/skadi \
  -eMAX_PARALLEL_PROCESSING=50 \
  -p3000:3000 racemap/elevation-service
```

### Example: Resource-Constrained Environment

For a resource-constrained environment like a small container with limited CPU and memory:

```bash
docker run --rm \
  -eTILE_SET_PATH=s3://elevation-tiles-prod/skadi \
  -eMAX_TOKIO_THREADS=2 \
  -eMAX_CONCURRENT_HANDLERS=50 \
  -eMAX_PARALLEL_PROCESSING=25 \
  -p3000:3000 racemap/elevation-service
```

## Usage with pre-downloaded data

Download data (ca. 200 GB):

```bash
aws s3 cp --no-sign-request --recursive s3://elevation-tiles-prod/skadi /path/to/data/folder
```

Run the docker container:

```bash
docker run --rm -v/path/to/data/folder:/app/data -p3000:3000 racemap/elevation-service
```

## Usage with S3-hosted data

### Public S3 buckets (HTTP access)

Run the docker container:

```bash
docker run --rm -eTILE_SET_PATH=s3://elevation-tiles-prod/skadi -p3000:3000 racemap/elevation-service
```

### Private S3 buckets with credentials

For private S3 buckets or when you want to use direct S3 API access with credentials:

```bash
docker run --rm \
  -eTILE_SET_PATH=s3://your-private-bucket/skadi \
  -eS3_ACCESS_KEY_ID=your_access_key \
  -eS3_SECRET_ACCESS_KEY=your_secret_key \
  -eS3_REGION=us-east-1 \
  -p3000:3000 racemap/elevation-service
```

### S3-compatible services (MinIO, DigitalOcean Spaces, etc.)

For S3-compatible services, you can specify a custom endpoint:

```bash
docker run --rm \
  -eTILE_SET_PATH=s3://your-bucket/skadi \
  -eS3_ACCESS_KEY_ID=your_access_key \
  -eS3_SECRET_ACCESS_KEY=your_secret_key \
  -eS3_ENDPOINT=your-endpoint.com \
  -p3000:3000 racemap/elevation-service
```

### Environment Variables

The following environment variables are supported for configuration:

#### General Configuration
- `TILE_SET_PATH`: Path to tiles (local path, HTTP/HTTPS URL, or s3:// URL)
- `TILE_SET_CACHE`: Cache size for tiles (default: 128)
- `MAX_POST_SIZE`: Maximum POST payload size (default: 500kb)
- `PORT`: Server port (default: 3000)
- `BIND`: Bind address (default: 0.0.0.0)

#### Resource Limiting
- `MAX_PARALLEL_PROCESSING`: Maximum parallel tile processing for batch requests (default: 500)
- `MAX_THREADS`: Maximum number of tokio runtime threads (optional, defaults to number of CPU cores)
- `MAX_CONCURRENT_HANDLERS`: Maximum number of concurrent request handlers using semaphore (default: 1000)

#### S3 Configuration
- `S3_ACCESS_KEY_ID`: S3 access key ID for authentication (also accepts `AWS_ACCESS_KEY_ID`)
- `S3_SECRET_ACCESS_KEY`: S3 secret access key for authentication (also accepts `AWS_SECRET_ACCESS_KEY`)
- `S3_REGION`: S3 region (default: us-east-1, also accepts `AWS_REGION`)
- `S3_ENDPOINT`: Custom S3 endpoint for S3-compatible services
- `S3_BUCKET`: S3 bucket name (alternative to specifying in TILE_SET_PATH)

## OpenTelemetry Telemetry

The elevation service now includes integrated OpenTelemetry (OTEL) telemetry for distributed tracing, structured logging, and performance monitoring. The implementation uses standard Rust crates and provides comprehensive observability.

### Features

- **Distributed Tracing**: Tracks requests through the entire service lifecycle
- **Structured Logging**: Enhanced logging with structured events and context
- **Performance Monitoring**: Automatic instrumentation of key operations
- **OTLP Export**: Sends telemetry data to OpenTelemetry-compatible backends
- **Fallback Support**: Gracefully falls back to basic tracing when OTLP is not configured

### Configuration

Configure telemetry using these environment variables:

- `SERVICE_NAME`: Service name for telemetry (default: "elevation-service")
- `OTEL_EXPORTER_OTLP_ENDPOINT`: OTLP endpoint URL (e.g., "http://jaeger:4317")
- `RUST_LOG`: Log level for tracing (e.g., "info", "debug", "trace")

### Running with Jaeger

For development and testing, use the provided development docker-compose file:

```bash
# Start Jaeger and the elevation service
docker compose -f docker-compose.dev.yaml up

# Access Jaeger UI
open http://localhost:16686
```

### Production Deployment

For production, set the environment variables in your deployment:

```bash
export SERVICE_NAME=elevation-service
export OTEL_EXPORTER_OTLP_ENDPOINT=http://your-jaeger-instance:4317
export RUST_LOG=info
```

### Instrumented Operations

The following operations are automatically traced:

- **HTTP Requests**: All incoming requests with method, path, and response status
- **Elevation Queries**: Single and batch elevation requests with coordinates
- **Status Checks**: Health check operations
- **Tile Loading**: HGT tile loading and caching operations
- **Error Handling**: Structured error logging with context

### Telemetry Data

The service generates the following types of telemetry data:

- **Spans**: Distributed traces for each operation
- **Events**: Structured log events with contextual information
- **Attributes**: Service metadata, coordinates, response times
- **Metrics**: Performance counters and operational metrics

When OTLP endpoint is not configured, the service falls back to console-based structured logging.

## License

MIT
