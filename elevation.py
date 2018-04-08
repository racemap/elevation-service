import math
import gdal
import osr
import json
from collections import namedtuple
from urllib import urlretrieve
from urllib2 import urlopen
from os import makedirs, remove
from os.path import exists, dirname

from lru import lru_cache_function

TILE_SIZE = 512

Point = namedtuple('Point', ['x', 'y'])
LatLng = namedtuple('LatLng', ['lat', 'lng'])

def project(latlng):
  siny = math.sin(latlng.lat * math.pi / 180)

  # Truncating to 0.9999 effectively limits latitude to 89.189. This is
  # about a third of a tile past the edge of the world tile.
  siny = min(max(siny, -0.9999), 0.9999)

  return Point(
    x=TILE_SIZE * (0.5 + latlng.lng / 360),
    y=TILE_SIZE * (0.5 - math.log((1 + siny) / (1 - siny)) / (4 * math.pi))
  )

def compute_tile(latlng, zoom):
  scale = 2 ** zoom

  worldCoordinate = project(latlng)

  pixelCoordinate = Point(
    x=math.floor(worldCoordinate.x * scale),
    y=math.floor(worldCoordinate.y * scale)
  )

  tileCoordinate = Point(
    x=math.floor(worldCoordinate.x * scale / TILE_SIZE),
    y=math.floor(worldCoordinate.y * scale / TILE_SIZE)
  )

  return tileCoordinate

def tile_filename(tile_coordinate, zoom):
  return ".terrain-cache/{}/{}/{}.tif".format(int(zoom), int(tile_coordinate.x), int(tile_coordinate.y))

def download_tile(tileCoordinate, zoom, filename):
  urlretrieve('https://elevation-tiles-prod.s3.amazonaws.com/geotiff/{}/{}/{}.tif'
    .format(int(zoom), int(tileCoordinate.x), int(tileCoordinate.y)), filename)
  print("Downloaded {}".format(filename))


class Lookup:
  def __init__(self, tile_coordinate, zoom):
    self.filename = tile_filename(tile_coordinate, zoom)
    if not exists(dirname(self.filename)):
        makedirs(dirname(self.filename))
    if not exists(self.filename):
        download_tile(tile_coordinate, zoom, self.filename)

    self.geotiff = gdal.Open(self.filename)
    geo_transform = self.geotiff.GetGeoTransform()

    spatial_reference_raster = osr.SpatialReference(self.geotiff.GetProjection())
    spatial_reference = osr.SpatialReference()
    spatial_reference.ImportFromEPSG(4326)  # WGS84
    self.coordinate_transform = osr.CoordinateTransformation(spatial_reference, spatial_reference_raster)

    dev = (geo_transform[1] * geo_transform[5] - geo_transform[2] * geo_transform[4])
    self.geo_transform_inv = (geo_transform[0], geo_transform[5] / dev, -geo_transform[2] / dev,geo_transform[3], -geo_transform[4] / dev, geo_transform[1] / dev)

    raster_band = self.geotiff.GetRasterBand(1)
    self.pixel_array = raster_band.ReadAsArray()

  def _lookup(self, latlng):
    xgeo, ygeo, zgeo = self.coordinate_transform.TransformPoint(latlng.lng, latlng.lat, 0)
    u = xgeo - self.geo_transform_inv[0]
    v = ygeo - self.geo_transform_inv[3]

    x = self.geo_transform_inv[1] * u + self.geo_transform_inv[2] * v
    y = self.geo_transform_inv[4] * u + self.geo_transform_inv[5] * v

    return self.pixel_array[math.floor(x), math.floor(y)]

  def __delete__(self):
    print("Removing {}".format(self.filename))
    remove(self.filename)

  @staticmethod
  def lookup(latlng, zoom=14):
    tile = compute_tile(latlng, zoom)
    return Lookup.get(tile, zoom)._lookup(latlng)

  @staticmethod
  @lru_cache_function(max_size=1024*8, expiration=2**60)
  def get(tile, zoom):
    return Lookup(tile, zoom)


if __name__ == "__main__":

  def get_open_elevation(latlng):
    obj = json.load(urlopen("https://api.open-elevation.com/api/v1/lookup?locations={},{}"
      .format(latlng.lat, latlng.lng)))
    return obj["results"][0]["elevation"]

  def do(latlng):
    ele = []
    for zoom in (10, 12, 14):
      ele.append(Lookup.lookup(latlng, zoom))
    return latlng, ele, get_open_elevation(latlng)

  print(do(LatLng(lat=52.3882084, lng=13.119842)))
  print(do(LatLng(lat=-85.05112, lng=179.999999)))
  print(do(LatLng(lat=85.05112, lng=-179.999999)))

  import numpy as np
  lngs = list(np.random.uniform(-179.99999999, 179.99999999, 100))
  lats = list(np.random.uniform(-85.05112, 85.05112, 100))

  for lat, lng in zip(lats, lngs):
    latlng = LatLng(lat=lat, lng=lng)
    print(do(latlng))

