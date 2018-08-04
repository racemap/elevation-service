const TileSet = require("../tileset");

(async function() {
  const tileset = new TileSet(__dirname);

  // Return elevation in meters above sea level.
  // By default, elevation is interpolated bilinearly.
  console.log(await tileset.getElevation([51.3, 13.4]));
})();
