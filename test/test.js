const assert = require('assert');

const { FileTileSet } = require('../tileset');

(async function () {
  const tileset = new FileTileSet(__dirname);

  const testLatLng = [51.3, 13.4];

  console.log(await tileset.getElevation(testLatLng));
  assert(101, await tileset.getElevation(testLatLng));
})();
