const path = require("path");
const { createReadStream } = require("fs");
const { get } = require("https");
const { promisify } = require("util");
const { createGunzip } = require("zlib");
const memoize = require("lru-memoize").default;

const HGT = require("./hgt");

class TileSet {
  constructor(options) {
    this.options = Object.assign(
      {},
      {
        cacheSize: 128
      },
      options
    );
    this.getTile = memoize(this.options.cacheSize)(this._getTile.bind(this));
  }

  getFilePath(lat, lng) {
    const latFileName = `${lat < 0 ? "S" : "N"}${String(Math.abs(lat)).padStart(
      2,
      "0"
    )}`;
    const lngFileName = `${lng < 0 ? "W" : "E"}${String(Math.abs(lng)).padStart(
      3,
      "0"
    )}`;
    const fileName = `${latFileName}${lngFileName}.hgt.gz`;
    return `${latFileName}/${fileName}`;
  }

  async getElevation(latLng) {
    const tile = await this.getTile(
      Math.floor(latLng[0]),
      Math.floor(latLng[1])
    );
    return tile.getElevation(latLng);
  }
}

class FileTileSet extends TileSet {
  constructor(folder, options) {
    super(
      Object.assign(
        {},
        {
          cacheSize: 128,
          gzip: true
        },
        options
      )
    );
    this._folder = folder;
  }

  async _getTile(lat, lng) {
    let stream = fs.createReadStream(
      path.join(this._folder, this.getFilePath(lat, lng))
    );
    if (this.options.gzip) {
      stream = stream.pipe(createGunzip());
    }
    const tile = await HGT.loadStream(stream, [lat, lng]);
    return tile;
  }
}

class S3TileSet extends TileSet {
  async _getTile(lat, lng) {
    console.log(`${S3TileSet.baseUrl}/${this.getFilePath(lat, lng)}`);
    let stream = await new Promise(resolve =>
      get(`${S3TileSet.baseUrl}/${this.getFilePath(lat, lng)}`, resolve)
    );
    if (this.options.gzip) {
      stream = stream.pipe(createGunzip());
    }
    const tile = await HGT.loadStream(stream, [lat, lng]);
    return tile;
  }
}
S3TileSet.baseUrl = "https://elevation-tiles-prod.s3.amazonaws.com/skadi";

TileSet.S3TileSet = S3TileSet;
TileSet.FileTileSet = FileTileSet;

module.exports = TileSet;
