//! Parse control names once, at the UI adapter; service messages are exhaustive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerAction {
    Toggle,
    Pause,
    Resume,
    Stop,
    Next,
    Previous,
    Shuffle,
    Repeat,
    QueuePreview,
    Letter(String),
}
impl PlayerAction {
    pub fn parse(action: &str) -> Result<Self, String> {
        Ok(match action {
            "toggle" => Self::Toggle,
            "pause" => Self::Pause,
            "resume" => Self::Resume,
            "stop" => Self::Stop,
            "next" => Self::Next,
            "previous" => Self::Previous,
            "shuffle" => Self::Shuffle,
            "repeat" => Self::Repeat,
            "queue-preview" => Self::QueuePreview,
            value if value.starts_with("letter-") && value[7..].chars().count() == 1 => {
                Self::Letter(value[7..].into())
            }
            _ => return Err("Unknown player action".into()),
        })
    }
    pub fn refresh_queue(&self) -> bool {
        matches!(
            self,
            Self::Shuffle | Self::Repeat | Self::Next | Self::Previous | Self::QueuePreview
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionAction {
    Play,
    AddToQueue,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueueAction {
    Add(String),
    Remove(String),
    Clear,
    Move { source: String, target: String },
    Play(String),
}
impl QueueAction {
    pub fn parse(action: &str, source: String, target: String) -> Result<Self, String> {
        Ok(match action {
            "add" => Self::Add(source),
            "remove" => Self::Remove(source),
            "clear" => Self::Clear,
            "move" => Self::Move { source, target },
            "play" => Self::Play(source),
            _ => return Err("Unknown queue action".into()),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_control_names_are_rejected_at_adapter() {
        for action in ["", "nex", "letter-", "letter-AB"] {
            assert!(PlayerAction::parse(action).is_err());
        }
        assert_eq!(
            PlayerAction::parse("letter-#").unwrap(),
            PlayerAction::Letter("#".into())
        );
        assert!(QueueAction::parse("unknown", "a".into(), "b".into()).is_err());
    }
}
