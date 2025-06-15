use super::{Agent, Task};

pub struct Modern {
    pub prompt: String,
    pub agent_type: Agent,
}

impl Modern {
    pub fn new() -> Self {
        Self {
            prompt: String::new(),
            agent_type: Agent::Modern,
        }
    }
}

impl Task for Modern {
    fn execute(self) {
        // Implementation will be added later
    }
}

impl Default for Modern {
    fn default() -> Self {
        Self::new()
    }
}
