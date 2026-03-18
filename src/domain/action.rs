#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerAction {
    Start,
    Stop,
    Kickstart,
}

impl TriggerAction {
    pub fn from_ui_value(value: &str) -> Option<Self> {
        match value {
            "start" => Some(TriggerAction::Start),
            "stop" => Some(TriggerAction::Stop),
            "kickstart" => Some(TriggerAction::Kickstart),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TriggerAction::Start => "start",
            TriggerAction::Stop => "stop",
            TriggerAction::Kickstart => "kickstart",
        }
    }
}
