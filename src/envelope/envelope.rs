use crate::errors::QightError;
use wincode::{SchemaRead, SchemaWrite};

#[repr(C)]
#[derive(SchemaRead, SchemaWrite, Debug,Clone)]
pub struct MessageEnvelope {
    pub msg_id: String,
    pub sender: String,
    pub recipient: String,
    pub timestamp: u64,
    pub ttl: u32,
    pub payload: Vec<u8>,
}

impl MessageEnvelope {
    pub fn new(
        sender: String,
        recipient: String,
        payload: Vec<u8>,
        ttl: u32,
    ) -> MessageEnvelope {
        let message_id = uuid::Uuid::new_v4().to_string();
        let message_object = MessageEnvelope {
            msg_id: message_id,
            sender,
            recipient,
            payload,
            timestamp: chrono::Utc::now().timestamp() as u64,
            ttl,
        };
        message_object
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, anyhow::Error> {
        let bytes = 
            wincode::serialize(self).map_err(|_| QightError::CannotSerializeBytes)?;
        Ok(bytes)
    }

    pub  fn from_bytes(&self, bytes: &[u8]) -> Result<MessageEnvelope, anyhow::Error> {
        let bytes: MessageEnvelope =
            wincode::deserialize(bytes).map_err(|_| QightError::CannotDeserialzeBytes)?;
        Ok(bytes)
    }

    pub fn is_expired(&self, current_time: u64) -> bool {
        let message_time = self.timestamp;
        if message_time > current_time + self.ttl as u64 {
            false
        } else {
            true
        }
    }

    pub fn display(&self) -> &MessageEnvelope{
        return self
    }
}
