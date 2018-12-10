const { json, send } = require("micro");
const limitedMap = require("limited-map");
const query = require("micro-query");
const cors = require("micro-cors")();
const { FileTileSet, S3TileSet } = require("./tileset");

const cacheSize = process.env.TILE_SET_CACHE || 128;
const tileFolder = process.env.TILE_SET_PATH || __dirname;
const maxPostSize = process.env.MAX_POST_SIZE || "500kb";
const maxParallelProcessing = 500;

const tiles = tileFolder.startsWith("s3://")
  ? new S3TileSet({ cacheSize })
  : new FileTileSet(tileFolder, { cacheSize });

async function handlePOST(req, res) {
  const payload = await json(req, { limit: maxPostSize });
  if (
    !payload ||
    !Array.isArray(payload) ||
    !payload.every(([lat, lng]) => Number.isFinite(lat) && Number.isFinite(lng))
  ) {
    return send(res, 400, {
      error:
        "Invalid Payload. Expected a JSON array with latitude-longitude pairs: [[lat, lng], ...]"
    });
  }

  const result = await limitedMap(
    payload,
    ll => tiles.getElevation(ll),
    maxParallelProcessing
  );
  return result;
}

async function handleGET(req, res) {
  const reqQuery = query(req);
  const lat = parseFloat(reqQuery.lat);
  const lng = parseFloat(reqQuery.lng);
  if (lat == null || !Number.isFinite(lat)) {
    return send(res, 400, {
      error:
        "Invalid Latitude. Expected a float number as query parameter: ?lat=12.3&lng=45.6"
    });
  }
  if (lng == null || !Number.isFinite(lng)) {
    return send(res, 400, {
      error:
        "Invalid Longitude. Expected a float number as query parameter: ?lat=12.3&lng=45.6"
    });
  }
  const result = await tiles.getElevation([lat, lng]);
  return result;
}

async function handleGETStatus(req, res) {
  return send(res, 200, "Ok");
}

async function handler(req, res) {
  switch (req.method) {
    case "POST":
      return handlePOST(req, res);
    case "GET":
      if (req.url == "/status") {
        return handleGETStatus(req, res);
      } else {
        return handleGET(req, res);
      }
    case "OPTIONS":
      send(res, 200, "");
      return;
    default:
      return send(res, 405, { error: "Only GET or POST allowed" });
  }
}

module.exports = cors(handler);
