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

The following environment variables are supported for S3 configuration:

- `TILE_SET_PATH`: Path to tiles (local path, HTTP/HTTPS URL, or s3:// URL)
- `S3_ACCESS_KEY_ID`: S3 access key ID for authentication (also accepts `AWS_ACCESS_KEY_ID`)
- `S3_SECRET_ACCESS_KEY`: S3 secret access key for authentication (also accepts `AWS_SECRET_ACCESS_KEY`)
- `S3_REGION`: S3 region (default: us-east-1, also accepts `AWS_REGION`)
- `S3_ENDPOINT`: Custom S3 endpoint for S3-compatible services
- `S3_BUCKET`: S3 bucket name (alternative to specifying in TILE_SET_PATH)
- `TILE_SET_CACHE`: Cache size for tiles (default: 128)
- `MAX_POST_SIZE`: Maximum POST payload size (default: 500kb)
- `MAX_PARALLEL_PROCESSING`: Maximum parallel tile processing (default: 500)
- `PORT`: Server port (default: 3000)
- `BIND`: Bind address (default: 0.0.0.0)

## License

MIT
