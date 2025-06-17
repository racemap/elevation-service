const { json, send } = require('micro');
const limitedMap = require('limited-map');
const query = require('micro-query');
const cors = require('micro-cors')();
const { FileTileSet, S3TileSet } = require('./tileset');

const cacheSize = process.env.TILE_SET_CACHE || 128;
const tileFolder = process.env.TILE_SET_PATH || __dirname;
const maxPostSize = process.env.MAX_POST_SIZE || '500kb';
const maxParallelProcessing = 500;

const tiles = tileFolder.startsWith('s3://')
  ? new S3TileSet({ cacheSize })
  : new FileTileSet(tileFolder, { cacheSize });

function validateLat(lat) {
  return lat != null && Number.isFinite(lat) && -90 <= lat && lat <= 90;
}

function validateLng(lng) {
  return lng != null && Number.isFinite(lng) && -180 <= lng && lng <= 180;
}

async function handlePOST(req, res) {
  const payload = await json(req, { limit: maxPostSize });
  if (
    !payload ||
    !Array.isArray(payload) ||
    !payload.every(
      (ll) => Array.isArray(ll) && ll.length === 2 && validateLat(ll[0]) && validateLng(ll[1]),
    )
  ) {
    return send(res, 400, {
      error:
        'Invalid Payload. Expected a JSON array with valid latitude-longitude pairs: [[lat, lng], ...]',
    });
  }

  const result = await limitedMap(payload, (ll) => tiles.getElevation(ll), maxParallelProcessing);
  return result;
}

async function handleGET(req, res) {
  const reqQuery = query(req);
  const lat = parseFloat(reqQuery.lat);
  const lng = parseFloat(reqQuery.lng);
  if (!validateLat(lat)) {
    return send(res, 400, {
      error: 'Invalid Latitude. Expected a float number as query parameter: ?lat=12.3&lng=45.6',
    });
  }
  if (!validateLng(lng)) {
    return send(res, 400, {
      error: 'Invalid Longitude. Expected a float number as query parameter: ?lat=12.3&lng=45.6',
    });
  }

  const result = await tiles.getElevation([lat, lng]);
  return result;
}

async function handleGETStatus(req, res) {
  try {
    // try to receive a test value
    const randomLng = Math.random() * 360 - 180;
    const randomLat = Math.random() * 180 - 90;

    await tiles.getElevation([randomLat, randomLng]);
    return send(res, 200, 'Ok');
  } catch (error) {
    console.error('Status Check Failed!');
    console.error(error);
    return send(res, 500, 'Error');
  }
}

async function handler(req, res) {
  try {
    switch (req.method) {
      case 'POST':
        return await handlePOST(req, res);
      case 'GET':
        if (req.url === '/status') {
          return await handleGETStatus(req, res);
        } else {
          return await handleGET(req, res);
        }
      case 'OPTIONS':
        send(res, 200, '');
        return;
      default:
        return send(res, 405, { error: 'Only GET or POST allowed' });
    }
  } catch (err) {
    console.error(err);
    return send(res, 500, 'Server error');
  }
}

module.exports = cors(handler);
