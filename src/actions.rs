use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    StepUp,
    StepDown,
    CyclePresets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Target {
    Focused,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub action: Action,
    pub target: Target,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_equality() {
        let c1 = Command { action: Action::StepUp, target: Target::Focused };
        let c2 = Command { action: Action::StepUp, target: Target::Focused };
        assert_eq!(c1, c2);
    }
}
