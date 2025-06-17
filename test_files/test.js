const assert = require('assert');
const http = require('http');
const { once } = require('events');

const { FileTileSet } = require('../tileset');
const handler = require('../index');

(async function () {
  const tileset = new FileTileSet(__dirname);
  const testLatLng = [51.3, 13.4];
  const elevation = await tileset.getElevation(testLatLng);
  console.log(elevation);
  assert.strictEqual(elevation, 101);

  const server = http.createServer(handler);
  server.listen(0);
  await once(server, 'listening');
  const port = server.address().port;

  const response = await fetch(`http://127.0.0.1:${port}/`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: '[null]',
  });
  assert.strictEqual(response.status, 400);
  server.close();
  console.log('Done');
})();
