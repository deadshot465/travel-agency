use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TransferPlan {
    pub routes: Vec<Route>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Route {
    pub from: String,
    pub to: String,
    pub by: TransferMethod,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RouteWithDuration {
    pub from: String,
    pub to: String,
    pub by: TransferMethod,
    pub duration: String,
    pub alternative: AlternativeTravelDuration,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AlternativeTravelDuration {
    pub by: TransferMethod,
    pub duration: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransferMethod {
    DriveOrTaxi,
    PublicTransport,
}
