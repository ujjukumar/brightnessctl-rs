use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i32)]
pub enum Action {
    StepUp = 1,
    StepDown = 2,
    CyclePresets = 3,
}

impl Action {
    pub fn from_i32(id: i32) -> Option<Self> {
        match id {
            1 => Some(Action::StepUp),
            2 => Some(Action::StepDown),
            3 => Some(Action::CyclePresets),
            _ => None,
        }
    }
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
    fn test_action_from_i32() {
        assert_eq!(Action::from_i32(1), Some(Action::StepUp));
        assert_eq!(Action::from_i32(2), Some(Action::StepDown));
        assert_eq!(Action::from_i32(3), Some(Action::CyclePresets));
        assert_eq!(Action::from_i32(0), None);
        assert_eq!(Action::from_i32(4), None);
    }
}
