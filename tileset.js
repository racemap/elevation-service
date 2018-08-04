const path = require("path");
const fs = require("fs");
const { createGunzip } = require("zlib");
const memoize = require("lru-memoize").default;

const HGT = require("./hgt");

class TileSet {
  constructor(folder, options) {
    this.options = Object.assign(
      {},
      {
        cacheSize: 128,
        gzip: true
      },
      options
    );
    this.getTile = memoize(this.options.cacheSize)(this._getTile.bind(this));
    this._folder = folder;
  }

  async _getTile(lat, lng) {
    const fileName =
      `${lat < 0 ? "S" : "N"}${String(Math.abs(lat)).padStart(2, "0")}` +
      `${lng < 0 ? "W" : "E"}${String(Math.abs(lng)).padStart(3, "0")}.hgt.gz`;

    let stream = fs.createReadStream(path.join(this._folder, fileName));
    if (this.options.gzip) {
      stream = stream.pipe(createGunzip());
    }
    const tile = await HGT.loadStream(stream, [lat, lng]);
    return tile;
  }

  async getElevation(latLng) {
    const tile = await this.getTile(
      Math.floor(latLng[0]),
      Math.floor(latLng[1])
    );
    return tile.getElevation(latLng);
  }
}

module.exports = TileSet;
