use std::sync::Arc;

use google_maps::{LatLng, prelude::TravelMode};

use crate::shared::structs::{
    agent::Language,
    google_maps::{Route, TransferMethod},
};

pub async fn get_latitude_and_longitude(
    route: &Route,
    language: Language,
    client: Arc<::google_maps::Client>,
) -> anyhow::Result<(LatLng, LatLng)> {
    let response_language = match language {
        Language::Chinese => ::google_maps::Language::ChineseTaiwan,
        Language::Japanese => ::google_maps::Language::Japanese,
        _ => ::google_maps::Language::EnglishUs,
    };

    let from_response = client
        .geocoding()
        .with_language(response_language)
        .with_address(&route.from)
        .execute()
        .await?;

    let to_response = client
        .geocoding()
        .with_language(response_language)
        .with_address(&route.to)
        .execute()
        .await?;

    let from_location = from_response
        .results
        .first()
        .map(|g| g.geometry.location)
        .unwrap_or_default();

    let to_location = to_response
        .results
        .first()
        .map(|g| g.geometry.location)
        .unwrap_or_default();

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
        TransferMethod::Drive => TravelMode::Driving,
        _ => TravelMode::Transit,
    };

    let direction_response = client
        .directions(from, to)
        .with_language(response_language)
        .with_alternatives(false)
        .with_travel_mode(travel_mode)
        .execute()
        .await?;

    let duration = direction_response
        .routes
        .first()
        .and_then(|route| route.legs.first())
        .map(|leg| leg.duration.text.clone())
        .unwrap_or_default();

    Ok(duration)
}
