const { json, send } = require("micro");
const limitedMap = require("limited-map");
const { FileTileSet, S3TileSet } = require("./tileset");

const cacheSize = process.env.TILE_SET_CACHE || 128;
const tileFolder = process.env.TILE_SET_PATH || __dirname;
const maxPostSize = process.env.MAX_POST_SIZE || "500kb";
const maxParallelProcessing = 500;

const tiles = tileFolder.startsWith("s3://")
  ? new S3TileSet({ cacheSize })
  : new FileTileSet(tileFolder, { cacheSize });

module.exports = async (req, res) => {
  if (req.method !== "POST") {
    return send(res, 405, { error: "Only POST allowed" });
  }

  const payload = await json(req, { limit: maxPostSize });
  if (!payload || Object.keys(payload).length === 0) {
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
};
