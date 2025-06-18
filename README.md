# Elevation service

Self-hosted elevation service that works with the [terrain data provided by Mapzen and Amazon AWS S3](https://registry.opendata.aws/terrain-tiles/). You can either pre-download the entire data on your server (ca. 200 GB) or access directly on S3 (for minimal latency from `us-east-1` region).

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

Run the docker container:

```bash
docker run --rm -eTILE_SET_PATH=s3://elevation-tiles-prod/skadi -p3000:3000 racemap/elevation-service
```

## License

MIT
