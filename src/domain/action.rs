#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerAction {
    Start,
    Stop,
    Kickstart,
    Enable,
    Disable,
    Load,
    Unload,
}

impl TriggerAction {
    pub fn from_ui_value(value: &str) -> Option<Self> {
        match value {
            "start" => Some(TriggerAction::Start),
            "stop" => Some(TriggerAction::Stop),
            "kickstart" | "restart" => Some(TriggerAction::Kickstart),
            "enable" => Some(TriggerAction::Enable),
            "disable" => Some(TriggerAction::Disable),
            "load" => Some(TriggerAction::Load),
            "unload" => Some(TriggerAction::Unload),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TriggerAction::Start => "start",
            TriggerAction::Stop => "stop",
            TriggerAction::Kickstart => "kickstart",
            TriggerAction::Enable => "enable",
            TriggerAction::Disable => "disable",
            TriggerAction::Load => "load",
            TriggerAction::Unload => "unload",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TriggerAction;

    #[test]
    fn restart_alias_maps_to_kickstart() {
        assert_eq!(
            TriggerAction::from_ui_value("restart"),
            Some(TriggerAction::Kickstart)
        );
    }
}
