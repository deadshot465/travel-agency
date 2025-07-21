use std::sync::Arc;

use dashmap::DashMap;
use google_maps::{
    LatLng,
    prelude::{DepartureTime, Local, TravelMode},
};

use crate::shared::structs::{
    agent::Language,
    google_maps::{AlternativeTravelDuration, Route, TransferMethod},
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
) -> anyhow::Result<(String, AlternativeTravelDuration)> {
    let response_language = match language {
        Language::Chinese => ::google_maps::Language::ChineseTaiwan,
        Language::Japanese => ::google_maps::Language::Japanese,
        _ => ::google_maps::Language::EnglishUs,
    };

    let (travel_mode, alternative_travel_mode) = match transfer_method {
        TransferMethod::DriveOrTaxi => (TravelMode::Driving, TravelMode::Transit),
        _ => (TravelMode::Transit, TravelMode::Driving),
    };

    let date = Local::now().date_naive();
    // To get approximate travel time from a place to another, we're setting time to 12:00:00 here.
    let departure_time = DepartureTime::At(
        date.and_time(
            chrono::NaiveTime::from_hms_opt(12, 0, 0)
                .ok_or(anyhow::anyhow!("Failed to construct a NaiveTime"))?,
        ),
    );

    let direction_response = client
        .directions(from, to)
        .with_language(response_language)
        .with_alternatives(false)
        .with_travel_mode(travel_mode.clone())
        .with_departure_time(departure_time.clone())
        .execute()
        .await;

    let alternative_direction_response = client
        .directions(from, to)
        .with_language(response_language)
        .with_alternatives(false)
        .with_travel_mode(alternative_travel_mode.clone())
        .with_departure_time(departure_time)
        .execute()
        .await;

    let alternative_transfer_method = match alternative_travel_mode {
        TravelMode::Driving => TransferMethod::DriveOrTaxi,
        _ => TransferMethod::PublicTransport,
    };

    match (direction_response, alternative_direction_response) {
        (Ok(res_1), Ok(res_2)) => Ok((
            extract_duration_text(&res_1.routes),
            AlternativeTravelDuration {
                by: alternative_transfer_method,
                duration: Some(extract_duration_text(&res_2.routes)),
            },
        )),
        (Ok(res_1), Err(e)) => {
            let error_msg = format!("Failed to get result for alternative route: {e:?}");
            tracing::warn!("{error_msg}");
            Ok((
                extract_duration_text(&res_1.routes),
                AlternativeTravelDuration {
                    by: alternative_transfer_method,
                    duration: None,
                },
            ))
        }
        (Err(e), Ok(res_2)) => {
            let error_msg = format!("Failed to get result for main route: {e:?}");
            tracing::warn!("{error_msg}");
            Ok((
                "No result".into(),
                AlternativeTravelDuration {
                    by: alternative_transfer_method,
                    duration: Some(extract_duration_text(&res_2.routes)),
                },
            ))
        }
        (Err(e_1), Err(e_2)) => {
            let error_msg =
                format!("Failed to get any result from API.\nError 1: {e_1:?}\nError 2: {e_2:?}");
            tracing::warn!("{error_msg}");
            Ok((
                "No result".into(),
                AlternativeTravelDuration {
                    by: alternative_transfer_method,
                    duration: None,
                },
            ))
        }
    }
}

fn extract_duration_text(routes: &[::google_maps::directions::response::route::Route]) -> String {
    routes
        .first()
        .and_then(|r| r.legs.first())
        .map(|l| l.duration.text.clone())
        .unwrap_or_default()
}
