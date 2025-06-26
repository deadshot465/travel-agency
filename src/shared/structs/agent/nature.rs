use super::{Agent, Taskable};

pub struct Nature {
    pub prompt: String,
    pub agent_type: Agent,
}

impl Nature {
    pub fn new() -> Self {
        Self {
            prompt: String::new(),
            agent_type: Agent::Nature,
        }
    }
}

impl Taskable for Nature {
    fn execute(self) {
        // Implementation will be added later
    }
}

impl Default for Nature {
    fn default() -> Self {
        Self::new()
    }
}
