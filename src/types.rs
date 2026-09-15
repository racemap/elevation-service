use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct LatLng {
    pub lat: f64,
    pub lng: f64,
}

#[derive(Deserialize)]
#[serde(transparent)]
pub struct LatLngs {
    pub latlngs: Vec<(f64, f64)>,
}

impl IntoIterator for LatLngs {
    type Item = (f64, f64);
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.latlngs.into_iter()
    }
}

#[derive(Serialize)]
#[serde(transparent)]
pub struct ElevationResponse {
    pub elevations: Vec<i16>,
}

/// Body of `/status`.
///
/// `status` is `ok` while the tile backend answers, `degraded` after a probe
/// failure that has not yet reached the threshold (still served as HTTP 200, so
/// a single transient object-storage blip does not page anyone), and
/// `unhealthy` once failures have been sustained (HTTP 500).
#[derive(Serialize, Debug, PartialEq)]
pub struct StatusResponse {
    pub status: &'static str,
    pub tileset: &'static str,
    pub consecutive_failures: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
