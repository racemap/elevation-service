use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct LatLng {
    pub lat: f64,
    pub lng: f64,
}

#[derive(Deserialize, Debug)]
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
