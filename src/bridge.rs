use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct BridgeEvent {
    pub event_type: EventType,
    pub author_name: String,
    pub author_avatar: String,
    pub channel_id: String,
    pub team_id: String,
}

#[derive(Debug, Clone)]
pub enum EventType {
    MessageSent {
        message_id: String,
        content: String,
    },
    MessageDeleted {
        message_id: String,
    },
    MessageEdited {
        message_id: String,
        new_content: String,
    },
    MessagePinned {
        message_id: String,
        content: String,
    },
    MessageUnpinned {
        message_id: String,
        content: String,
    },
}

#[derive(Clone)]
pub struct BridgeChannels {
    pub to_discord: mpsc::UnboundedSender<BridgeEvent>,
    pub to_slack: mpsc::UnboundedSender<BridgeEvent>,
}

pub fn create_bridge() -> (
    BridgeChannels,
    mpsc::UnboundedReceiver<BridgeEvent>, // discord receiver
    mpsc::UnboundedReceiver<BridgeEvent>, // slack receiver
) {
    let (discord_tx, discord_rx) = mpsc::unbounded_channel();
    let (slack_tx, slack_rx) = mpsc::unbounded_channel();

    let channels = BridgeChannels {
        to_discord: discord_tx,
        to_slack: slack_tx,
    };

    (channels, discord_rx, slack_rx)
}
