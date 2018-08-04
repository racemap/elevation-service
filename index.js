const { json, send } = require("micro");
const { S3TileSet } = require("./tileset");

const cacheSize = process.env.TILE_SET_CACHE || 128;
const tileFolder = process.env.TILE_SET_PATH || __dirname;
const maxPostSize = process.env.MAX_POST_SIZE || "500kb";

const tiles = new S3TileSet({ cacheSize });

module.exports = async (req, res) => {
  if (req.method !== "POST") {
    return send(res, 405, { error: "Only POST allowed" });
  }

  const geojson = await json(req, { limit: maxPostSize });
  if (!geojson || Object.keys(geojson).length === 0) {
    return send(res, 400, { error: "Invalid GeoJSON" });
  }

  const result = await Promise.all(geojson.map(ll => tiles.getElevation(ll)));
  return result;
};
