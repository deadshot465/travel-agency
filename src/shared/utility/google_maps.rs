use std::sync::Arc;

use dashmap::DashMap;
use google_maps::{LatLng, prelude::TravelMode};

use crate::shared::structs::{
    agent::Language,
    google_maps::{Route, TransferMethod},
};

pub async fn get_latitude_and_longitude(
    route: &Route,
    language: Language,
    lat_lngs: Arc<DashMap<String, LatLng>>,
    client: Arc<::google_maps::Client>,
) -> anyhow::Result<(LatLng, LatLng)> {
    let response_language = match language {
        Language::Chinese => ::google_maps::Language::ChineseTaiwan,
        Language::Japanese => ::google_maps::Language::Japanese,
        _ => ::google_maps::Language::EnglishUs,
    };

    let from_location = if let Some(lat_lng) = lat_lngs.get(&route.from) {
        *lat_lng
    } else {
        let from_response = client
            .geocoding()
            .with_language(response_language)
            .with_address(&route.from)
            .execute()
            .await?;

        let location = from_response
            .results
            .first()
            .map(|g| g.geometry.location)
            .unwrap_or_default();

        lat_lngs.insert(route.from.clone(), location);
        location
    };

    let to_location = if let Some(lat_lng) = lat_lngs.get(&route.to) {
        *lat_lng
    } else {
        let to_response = client
            .geocoding()
            .with_language(response_language)
            .with_address(&route.to)
            .execute()
            .await?;

        let location = to_response
            .results
            .first()
            .map(|g| g.geometry.location)
            .unwrap_or_default();

        lat_lngs.insert(route.to.clone(), location);
        location
    };

    Ok((from_location, to_location))
}

pub async fn get_travel_time(
    (from, to, transfer_method): (LatLng, LatLng, TransferMethod),
    language: Language,
    client: Arc<::google_maps::Client>,
) -> anyhow::Result<String> {
    let response_language = match language {
        Language::Chinese => ::google_maps::Language::ChineseTaiwan,
        Language::Japanese => ::google_maps::Language::Japanese,
        _ => ::google_maps::Language::EnglishUs,
    };

    let travel_mode = match transfer_method {
        TransferMethod::DriveOrTaxi => TravelMode::Driving,
        _ => TravelMode::Transit,
    };

    let direction_response = client
        .directions(from, to)
        .with_language(response_language)
        .with_alternatives(false)
        .with_travel_mode(travel_mode.clone())
        .execute()
        .await;

    match direction_response {
        Ok(res) => Ok(res
            .routes
            .first()
            .and_then(|route| route.legs.first())
            .map(|leg| leg.duration.text.clone())
            .unwrap_or_default()),
        Err(e) => {
            let error_message = format!("Search failed with {e:?}. Retry with driving...");
            tracing::warn!("{error_message}");

            let response = client
                .directions(from, to)
                .with_language(response_language)
                .with_alternatives(false)
                .with_travel_mode(travel_mode)
                .execute()
                .await;

            match response {
                Ok(res) => Ok(res
                    .routes
                    .first()
                    .and_then(|route| route.legs.first())
                    .map(|leg| leg.duration.text.clone())
                    .unwrap_or_default()),
                Err(e) => {
                    let error_message =
                        format!("Search failed with {e:?}. Returning empty results...");
                    tracing::warn!("{error_message}");
                    Ok("No result".into())
                }
            }
        }
    }
}
