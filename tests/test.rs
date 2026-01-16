#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::{self, Engine as _};
    use qight::*;
    use uuid::Uuid;

    #[test]
    fn test_envelope_creation() {
        let client = Uuid::new_v4().to_string();
        let sender_id = Uuid::new_v4().to_string();

        let envelope =
            MessageEnvelope::new(sender_id, client, BASE64.encode("sample").into_bytes(), 10);
        let converted_bytes = envelope.to_bytes().unwrap();

        println!("{:?}", converted_bytes);
        let deserialized_envelope = envelope.from_bytes(converted_bytes.as_slice()).unwrap();
        assert_eq!(envelope.msg_id, deserialized_envelope.msg_id);
        assert_eq!(envelope.payload, deserialized_envelope.payload);
        assert_eq!(envelope.sender, deserialized_envelope.sender);
        assert_eq!(envelope.timestamp, deserialized_envelope.timestamp);
        println!("{:?}", deserialized_envelope)
    }
}
