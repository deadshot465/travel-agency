use serde::{Deserialize, Serialize};

pub mod food;
pub mod history;
pub mod modern;
pub mod nature;
pub mod transport;

pub trait Taskable {
    fn execute(self);
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Agent {
    Food,
    Transport,
    History,
    Modern,
    Nature,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Japanese,
    Chinese,
    Other,
}

#[derive(Deserialize, Serialize)]
pub struct LanguageTriageArgumants {
    pub language: Language,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct OrchestrationResponse {
    pub analysis: String,
    pub greeting_message: String,
    pub synthesis_plan: String,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub task_id: String,
    pub agent: Agent,
    pub dependencies: Vec<String>,
    pub instruction: String,
}
