use super::{Agent, Task};

pub struct Food {
    pub prompt: String,
    pub agent_type: Agent,
}

impl Food {
    pub fn new() -> Self {
        Self {
            prompt: String::new(),
            agent_type: Agent::Food,
        }
    }
}

impl Task for Food {
    fn execute(self) {
        // Implementation will be added later
    }
}

impl Default for Food {
    fn default() -> Self {
        Self::new()
    }
}
