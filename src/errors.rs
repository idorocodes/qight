
use thiserror::Error;
use wincode::{SchemaRead, SchemaWrite};

#[derive(Error,Debug, SchemaRead,SchemaWrite)]
pub enum QightError{
    #[error("Cannot serialize message to bytes!")]
    CannotSerializeBytes,
    #[error("Cannot deserialize from bytes!")]
    CannotDeserialzeBytes,
}



