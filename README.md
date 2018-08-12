# elevation-service

[![CircleCI](https://circleci.com/gh/normanrz/elevation-service.svg?style=svg)](https://circleci.com/gh/normanrz/elevation-service)

Elevation service that works with the [Terrain data provided on Amazon AWS S3](https://registry.opendata.aws/terrain-tiles/). You can either pre-download the data on your server (ca. 200 GB) or access directly on S3 (for minimal latency from `us-east-1` region).

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

## Usage with pre-downloaded data

Download data (ca. 200 GB):

```bash
aws s3 cp --recursive s3://elevation-tiles-prod/skadi /path/to/data/folder
```

Run the docker container:

```bash
docker run --rm -v/path/to/data/folder:/app/data -p3000:3000 normanrz/elevation-service
```

## Usage with S3-hosted data

Run the docker container:

```bash
docker run --rm -eTILE_SET_PATH=s3:// -p3000:3000 normanrz/elevation-service
```

## License

MIT
