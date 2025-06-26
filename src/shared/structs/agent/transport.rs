use super::{Agent, Taskable};

pub struct Transport {
    pub prompt: String,
    pub agent_type: Agent,
}

impl Transport {
    pub fn new() -> Self {
        Self {
            prompt: String::new(),
            agent_type: Agent::Transport,
        }
    }
}

impl Taskable for Transport {
    fn execute(self) {
        // Implementation will be added later
    }
}

impl Default for Transport {
    fn default() -> Self {
        Self::new()
    }
}
