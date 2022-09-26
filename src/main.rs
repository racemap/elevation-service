use async_compression::tokio::bufread::GzipDecoder;
use byteorder::{ReadBytesExt, BE};
use cached::proc_macro::cached;
use futures::{self, StreamExt};
use lazy_static::lazy_static;
use serde::{self, Deserialize};
use std::convert::Infallible;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use tokio;
use tokio::io::{AsyncReadExt, BufReader};
use warp::{http::StatusCode, Filter, Rejection, Reply};

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;
pub type BoxResult<T> = std::result::Result<T, BoxError>;
pub type BoxUnitResult = std::result::Result<(), BoxError>;

#[allow(non_upper_case_globals)]
const UnitOk: BoxUnitResult = Ok(());

lazy_static! {
    pub static ref PORT: u16 = std::env::var("PORT")
        .ok()
        .and_then(|port| port.parse::<u16>().ok())
        .unwrap_or(3000);
    pub static ref TILE_SET_PATH: PathBuf = std::env::var("TILE_SET_PATH")
        .map(|path| PathBuf::from(path))
        .unwrap_or_else(|_| std::env::current_dir().unwrap());
    pub static ref MAX_PARALLEL: usize = std::env::var("MAX_PARALLEL")
        .ok()
        .and_then(|max_parallel| max_parallel.parse::<usize>().ok())
        .unwrap_or(500);
}

#[derive(Debug)]
pub struct StringError {
    msg: String,
}
impl StringError {
    pub fn new(s: &str) -> Self {
        Self {
            msg: String::from(s),
        }
    }
    pub fn boxed(s: &str) -> Box<Self> {
        Box::new(Self {
            msg: String::from(s),
        })
    }
}
impl std::error::Error for StringError {}
impl core::fmt::Display for StringError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

#[derive(Copy, Clone, Debug, Deserialize)]
pub struct RawGetQuery {
    lng: f64,
    lat: f64,
}

#[derive(Copy, Clone, Debug, Deserialize)]
pub struct RawPostRequestBodyItem(pub f64, pub f64);

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub struct LngLat {
    pub lng: f64,
    pub lat: f64,
}
impl TryFrom<RawGetQuery> for LngLat {
    type Error = StringError;
    fn try_from(query: RawGetQuery) -> Result<Self, Self::Error> {
        if query.lat.is_finite() && -90.0 <= query.lat && query.lat <= 90.0 {
            if query.lng.is_finite() && -180.0 <= query.lng && query.lng <= 180.0 {
                Ok(LngLat {
                    lng: query.lng,
                    lat: query.lat,
                })
            } else {
                Err(StringError::new("Invalid longitude. Expected a float number as query parameter: ?lat=12.3&lng=45.6"))
            }
        } else {
            Err(StringError::new(
                "Invalid latitude. Expected a float number as query parameter: ?lat=12.3&lng=45.6",
            ))
        }
    }
}
impl TryFrom<RawPostRequestBodyItem> for LngLat {
    type Error = StringError;
    fn try_from(item: RawPostRequestBodyItem) -> Result<Self, Self::Error> {
        let lat = item.0;
        let lng = item.1;
        if lat.is_finite() && -90.0 <= lat && lat <= 90.0 {
            if lng.is_finite() && -180.0 <= lng && lng <= 180.0 {
                return Ok(LngLat { lng, lat });
            }
        }
        Err(StringError::new("Invalid Payload. Expected a JSON array with valid latitude-longitude pairs: [[lat, lng], ...]"))
    }
}
impl Hash for LngLat {
    fn hash<H: Hasher>(&self, state: &mut H) {
        ((self.lng * 100_000.0) as i64).hash(state);
        ((self.lat * 100_000.0) as i64).hash(state);
    }
}
impl Eq for LngLat {}
impl LngLat {
    pub fn floor(self) -> Self {
        Self {
            lng: self.lng.floor(),
            lat: self.lat.floor(),
        }
    }
}

async fn health_handler() -> Result<impl Reply, Infallible> {
    Ok("Ok.")
}
async fn get_handler(tileset: Tileset, raw_lnglat: RawGetQuery) -> Result<impl Reply, Rejection> {
    let lnglat = match LngLat::try_from(raw_lnglat) {
        Ok(lnglat) => lnglat,
        Err(_) => {
            return Err(warp::reject::custom(InvalidQuery));
        }
    };
    match tileset.get_elevation(lnglat).await {
        Ok(elevation) => Ok(warp::reply::json(&elevation)),
        Err(_) => Err(warp::reject::custom(ServerError)),
    }
}
async fn post_handler(
    tileset: Tileset,
    raw_lnglats: Vec<RawPostRequestBodyItem>,
) -> Result<impl Reply, Rejection> {
    let lnglats = match raw_lnglats.into_iter().map(LngLat::try_from).collect() {
        Ok(lnglats) => lnglats,
        Err(_) => {
            return Err(warp::reject::custom(InvalidQuery));
        }
    };
    match tileset.get_elevations(lnglats).await {
        Ok(elevations) => Ok(warp::reply::json(&elevations)),
        Err(_) => Err(warp::reject::custom(ServerError)),
    }
}

#[derive(Clone)]
pub struct Tile {
    buf: Vec<u8>,
    lnglat: LngLat,
    size: usize,
}
impl Tile {
    async fn read_gzip_file(filename: &Path) -> BoxResult<Vec<u8>> {
        let f = BufReader::new(tokio::fs::File::open(filename).await?);
        let mut buf = Vec::new();
        GzipDecoder::new(f).read_to_end(&mut buf).await?;
        Ok(buf)
    }

    pub async fn new(lnglat: LngLat, filename: &Path) -> BoxResult<Self> {
        let buf = Self::read_gzip_file(filename).await?;
        let size = if buf.len() == 12967201 * 2 {
            3601
        } else if buf.len() == 1442401 * 2 {
            1201
        } else {
            return Err(StringError::boxed(
                "Unknown tile format (1 arcsecond and 3 arcsecond supported).",
            ));
        };

        Ok(Self { buf, lnglat, size })
    }

    fn read(&self, row: usize, col: usize) -> BoxResult<i16> {
        let mut cursor = Cursor::new(&self.buf);
        cursor.set_position((((self.size - row - 1) * self.size + col) * 2) as u64);
        Ok(ReadBytesExt::read_i16::<BE>(&mut cursor)?)
    }

    pub fn get_elevation(&self, lnglat: LngLat) -> BoxResult<f64> {
        let row = (lnglat.lat - self.lnglat.lat) * (self.size - 1) as f64;
        let col = (lnglat.lng - self.lnglat.lng) * (self.size - 1) as f64;

        if row < 0.0 || col < 0.0 || row >= self.size as f64 || col >= self.size as f64 {
            return Err(StringError::boxed(&format!(
                "Latitude/longitude is outside tile bounds (row={}, col={}; size={})",
                row, col, self.size
            )));
        }

        let row_low = row.floor() as usize;
        let row_hi = row_low + 1;
        let row_frac = row - row_low as f64;
        let col_low = col.floor() as usize;
        let col_hi = col_low + 1;
        let col_frac = col - col_low as f64;
        let v00 = self.read(row_low, col_low)?;
        let v10 = self.read(row_low, col_hi)?;
        let v11 = self.read(row_hi, col_hi)?;
        let v01 = self.read(row_hi, col_low)?;
        let v1 = avg(v00 as f64, v10 as f64, col_frac);
        let v2 = avg(v01 as f64, v11 as f64, col_frac);

        Ok(avg(v1, v2, row_frac))
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Tileset {
    path: PathBuf,
}
impl Tileset {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_owned(),
        }
    }

    pub fn get_filename(&self, lnglat: LngLat) -> PathBuf {
        let lat_filename = format!(
            "{}{:02}",
            if lnglat.lat < 0.0 { "S" } else { "N" },
            lnglat.lat.abs()
        );
        let lng_filename = format!(
            "{}{:03}",
            if lnglat.lng < 0.0 { "W" } else { "E" },
            lnglat.lng.abs()
        );
        let mut path_buf = PathBuf::new();
        path_buf.push(&self.path);
        path_buf.push(&lat_filename);
        path_buf.push(format!("{}{}.hgt.gz", &lat_filename, &lng_filename));
        path_buf
    }

    async fn get_tile(&self, lnglat: LngLat) -> BoxResult<Tile> {
        Ok(load_tile(self.clone(), lnglat.floor()).await?)
    }

    pub async fn get_elevation(&self, lnglat: LngLat) -> BoxResult<f64> {
        let tile = self.get_tile(lnglat).await?;
        Ok(tile.get_elevation(lnglat)?)
    }

    pub async fn get_elevations(&self, lnglats: Vec<LngLat>) -> BoxResult<Vec<f64>> {
        let results = futures::stream::iter(lnglats.into_iter().map(|lnglat| {
            let tileset = self.clone();
            tokio::spawn(async move { Ok::<f64, BoxError>(tileset.get_elevation(lnglat).await?) })
        }))
        .buffer_unordered(*MAX_PARALLEL)
        .collect::<Vec<_>>()
        .await;

        let mut output = Vec::with_capacity(results.len());
        for elv in results.into_iter() {
            match elv {
                Ok(Ok(elv)) => {
                    output.push(elv);
                }
                Err(err) => {
                    log::error!("{:?}", err);
                    return Err(StringError::boxed("Couldn't fetch elevation"));
                }
                Ok(Err(err)) => {
                    log::error!("{:?}", err);
                    return Err(StringError::boxed("Couldn't fetch elevation"));
                }
            }
        }
        Ok(output)
    }
}

#[cached(sync_writes = true, result = true)]
async fn load_tile(tileset: Tileset, lnglat: LngLat) -> BoxResult<Tile> {
    Ok(Tile::new(lnglat, &tileset.get_filename(lnglat)).await?)
}

fn avg(v1: f64, v2: f64, f: f64) -> f64 {
    v1 + (v2 - v1) * f
}

#[derive(Debug)]
struct InvalidQuery;
impl warp::reject::Reject for InvalidQuery {}

#[derive(Debug)]
struct ServerError;
impl warp::reject::Reject for ServerError {}

async fn handle_rejection(err: Rejection) -> Result<impl Reply, std::convert::Infallible> {
    if err.is_not_found() {
        Ok(warp::reply::with_status("NOT_FOUND", StatusCode::NOT_FOUND))
    } else if let Some(_) = err.find::<InvalidQuery>() {
        Ok(warp::reply::with_status(
            "BAD_REQUEST",
            StatusCode::BAD_REQUEST,
        ))
    } else {
        Ok(warp::reply::with_status(
            "INTERNAL_SERVER_ERROR",
            StatusCode::INTERNAL_SERVER_ERROR,
        ))
    }
}

#[tokio::main]
async fn main() -> BoxUnitResult {
    pretty_env_logger::init();
    let logger = warp::log("elevation");

    let tileset = Tileset::new(TILE_SET_PATH.clone());

    let health_route = warp::path!("health")
        .and(warp::get())
        .and_then(health_handler);
    let get_route = warp::path!()
        .and(warp::get())
        .and({
            let tileset = tileset.clone();
            warp::any().map(move || tileset.clone())
        })
        .and(warp::query())
        .and_then(get_handler);
    let post_route = warp::path!()
        .and(warp::post())
        .and({
            let tileset = tileset.clone();
            warp::any().map(move || tileset.clone())
        })
        .and(warp::body::json())
        .and_then(post_handler);

    let cors = warp::cors().allow_any_origin();

    let routes = health_route
        .or(get_route)
        .or(post_route)
        .with(logger)
        .with(cors)
        .recover(handle_rejection);

    log::info!("Starting server on port {}", *PORT);
    warp::serve(routes).run(([0, 0, 0, 0], *PORT)).await;

    UnitOk
}

#[cfg(test)]
mod tests {
    use super::{BoxUnitResult, LngLat, Tileset, UnitOk};

    #[tokio::test]
    async fn can_fetch_elevations() -> BoxUnitResult {
        let tileset = Tileset::new(std::env::current_dir().unwrap());

        let elvs = tileset
            .get_elevations(vec![
                LngLat {
                    lng: 13.4,
                    lat: 51.3,
                },
                LngLat {
                    lng: 13.3,
                    lat: 51.4,
                },
            ])
            .await?;
        assert_eq!(vec![101.0, 100.0], elvs);

        let elv = tileset
            .get_elevation(LngLat {
                lng: 13.4,
                lat: 51.3,
            })
            .await?;
        assert_eq!(101.0, elv);
        UnitOk
    }
}
