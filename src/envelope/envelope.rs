use crate::{errors::QightError,keys_auth::key_fn::gen_key};
use ed25519_dalek::PUBLIC_KEY_LENGTH;
use wincode::{SchemaRead, SchemaWrite};

#[repr(C)]
#[derive(SchemaRead, SchemaWrite, Debug,Clone)]
pub struct MessageEnvelope {
    pub msg_id: [u8;PUBLIC_KEY_LENGTH],
    pub sender: String,
    pub sender_key:[u8;PUBLIC_KEY_LENGTH],
    pub recipient: [u8;PUBLIC_KEY_LENGTH],
    pub timestamp: u64,
    pub ttl: u32,
    pub payload: Vec<u8>,
}





impl MessageEnvelope {
    pub fn new(
        sender: String,
        recipient: [u8;PUBLIC_KEY_LENGTH],
        sender_key :[u8;PUBLIC_KEY_LENGTH],
        payload: Vec<u8>,
        ttl: u32,
    ) -> MessageEnvelope {
        let message_id = gen_key();
        let message_object = MessageEnvelope {
            msg_id: message_id,
            sender,
            sender_key,
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

    pub  fn from_bytes(bytes: &[u8]) -> Result<MessageEnvelope, anyhow::Error> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys_auth::gen_keypair;

    #[test]
    fn test_envelope_serialization() {
        let (recipient_key, _) = gen_keypair();
        let (sender_key, _) = gen_keypair();
        let original = MessageEnvelope::new(
            "test_sender".to_string(),
            recipient_key,
            sender_key,
            b"test payload".to_vec(),
            3600,
        );

        let bytes = original.to_bytes().unwrap();
        let deserialized = MessageEnvelope::from_bytes(&bytes).unwrap();

        assert_eq!(original.msg_id, deserialized.msg_id);
        assert_eq!(original.sender, deserialized.sender);
        assert_eq!(original.recipient, deserialized.recipient);
        assert_eq!(original.payload, deserialized.payload);
    }

    #[test]
    fn test_envelope_expiration() {
        let (recipient_key, _) = gen_keypair();
        let (sender_key, _) = gen_keypair();
        let envelope = MessageEnvelope::new(
            "test".to_string(),
            recipient_key,
            sender_key,
            vec![],
            100, // 100 seconds TTL
        );

        let current_time = envelope.timestamp + 50; // Halfway through TTL
        assert!(!envelope.is_expired(current_time));

        let expired_time = envelope.timestamp + 150; // Past TTL
        assert!(envelope.is_expired(expired_time));
    }

    #[test]
    fn test_envelope_from_bytes_invalid() {
        let invalid_bytes = b"not an envelope";
        assert!(MessageEnvelope::from_bytes(invalid_bytes).is_err());
    }
}