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
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransferMethod {
    Drive,
    Transit,
}
