use super::{Agent, Task};

pub struct History {
    pub prompt: String,
    pub agent_type: Agent,
}

impl History {
    pub fn new() -> Self {
        Self {
            prompt: String::new(),
            agent_type: Agent::History,
        }
    }
}

impl Task for History {
    fn execute(self) {
        // Implementation will be added later
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}
