//! IP geolocation — determine location from public IP.

use crate::types::GeoPoint;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

const LOCATION_URL: &str = "https://ipwho.is/?fields=success,latitude,longitude";
const CACHE_SOURCE: &str = "https://ipwho.is/";

#[derive(Deserialize)]
struct LocationResponse {
    success: bool,
    latitude: f64,
    longitude: f64,
}

#[derive(Deserialize, Serialize)]
struct CachedLocation {
    source: String,
    #[serde(flatten)]
    point: GeoPoint,
}

fn location_client() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(5))
}

async fn fetch_location(client: &reqwest::Client, endpoint: &str) -> Result<GeoPoint> {
    let response = client.get(endpoint).send().await?;
    ensure!(
        response.status().is_success(),
        "IP geolocation request failed"
    );
    let response: LocationResponse = response.json().await?;
    ensure!(
        response.success,
        "IP geolocation provider rejected the lookup"
    );
    let point = GeoPoint {
        lat: response.latitude,
        lon: response.longitude,
        alt: 0.0,
    };
    validate_location(&point)?;
    Ok(point)
}

fn validate_location(point: &GeoPoint) -> Result<()> {
    ensure!(
        point.lat.is_finite()
            && point.lon.is_finite()
            && point.alt.is_finite()
            && (-90.0..=90.0).contains(&point.lat)
            && (-180.0..=180.0).contains(&point.lon),
        "IP geolocation returned invalid coordinates"
    );
    ensure!(
        point.lat != 0.0 || point.lon != 0.0,
        "IP geolocation returned (0,0)"
    );
    Ok(())
}

fn cached_location(data: &str) -> Option<GeoPoint> {
    let cache: CachedLocation = serde_json::from_str(data).ok()?;
    if cache.source != CACHE_SOURCE || validate_location(&cache.point).is_err() {
        return None;
    }
    Some(cache.point)
}

/// Locate by public IP over verified HTTPS (free endpoint, no API key).
pub async fn locate_by_ip() -> Result<GeoPoint> {
    fetch_location(&location_client().build()?, LOCATION_URL).await
}

/// Get location with caching.
pub async fn get_location(cache_path: &str) -> Result<GeoPoint> {
    // Legacy bare GeoPoint files may contain data obtained over HTTP.
    if let Ok(data) = std::fs::read_to_string(cache_path) {
        if let Some(point) = cached_location(&data) {
            return Ok(point);
        }
    }

    let point = locate_by_ip().await?;
    let cache = CachedLocation {
        source: CACHE_SOURCE.to_owned(),
        point: point.clone(),
    };
    let _ = std::fs::write(cache_path, serde_json::to_string(&cache)?);
    Ok(point)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_coordinates_without_rejecting_equator_or_prime_meridian() {
        for (lat, lon) in [(43.0, -79.0), (0.0, 12.0), (12.0, 0.0), (-90.0, 180.0)] {
            assert!(validate_location(&GeoPoint { lat, lon, alt: 0.0 }).is_ok());
        }
        for (lat, lon) in [(91.0, 0.0), (0.0, -181.0), (0.0, 0.0), (f64::NAN, 12.0)] {
            assert!(validate_location(&GeoPoint { lat, lon, alt: 0.0 }).is_err());
        }
    }

    #[test]
    fn cache_requires_current_https_provenance_and_valid_coordinates() {
        assert!(cached_location(r#"{"lat":43,"lon":-79,"alt":0}"#).is_none());
        assert!(
            cached_location(r#"{"source":"http://ip-api.com/","lat":43,"lon":-79,"alt":0}"#)
                .is_none()
        );
        assert!(
            cached_location(r#"{"source":"https://ipwho.is/","lat":91,"lon":-79,"alt":0}"#)
                .is_none()
        );
        let point = cached_location(r#"{"source":"https://ipwho.is/","lat":43,"lon":-79,"alt":0}"#)
            .unwrap();
        assert_eq!((point.lat, point.lon, point.alt), (43.0, -79.0, 0.0));
    }

    #[test]
    fn new_cache_preserves_geopoint_fields_for_existing_readers() {
        let cache = CachedLocation {
            source: CACHE_SOURCE.to_owned(),
            point: GeoPoint {
                lat: 43.0,
                lon: -79.0,
                alt: 0.0,
            },
        };
        let data = serde_json::to_string(&cache).unwrap();
        let point: GeoPoint = serde_json::from_str(&data).unwrap();
        assert_eq!((point.lat, point.lon), (43.0, -79.0));
        assert!(cached_location(&data).is_some());
    }

    #[tokio::test]
    async fn rejects_plain_http_before_connecting() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/", listener.local_addr().unwrap());
        let error = fetch_location(&location_client().no_proxy().build().unwrap(), &endpoint)
            .await
            .unwrap_err();
        assert!(error.downcast_ref::<reqwest::Error>().unwrap().is_builder());
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}
