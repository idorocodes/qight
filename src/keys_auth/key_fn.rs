use ed25519_dalek::{
    PUBLIC_KEY_LENGTH, SECRET_KEY_LENGTH, SigningKey, Signer, Verifier, Signature, VerifyingKey, SIGNATURE_LENGTH,
};
use rand::rngs::OsRng;


pub fn gen_key() -> [u8;PUBLIC_KEY_LENGTH]{
    let mut cspring = OsRng;
    let signing_key: SigningKey = SigningKey::generate(&mut cspring);
    let pub_key: [u8; SECRET_KEY_LENGTH] = signing_key.to_bytes();
    pub_key
}   

pub fn gen_keypair() -> ([u8;PUBLIC_KEY_LENGTH] , [u8;SECRET_KEY_LENGTH]){
    let mut cspring = OsRng;
    let signing_key: SigningKey = SigningKey::generate(&mut cspring);
    let public_key: [u8; PUBLIC_KEY_LENGTH] = *signing_key.verifying_key().as_bytes();
    let private_key: [u8; SECRET_KEY_LENGTH] = signing_key.to_bytes();
    return (public_key,private_key);

}
pub fn sign_message(private_key: &[u8; SECRET_KEY_LENGTH], message: &[u8]) -> [u8; SIGNATURE_LENGTH] {
    let sk = SigningKey::from_bytes(private_key);
    let sig = sk.sign(message);
    sig.to_bytes()
}

pub fn verify_message(public_key: &[u8; PUBLIC_KEY_LENGTH], message: &[u8], signature: &[u8; SIGNATURE_LENGTH]) -> bool {
    if let Ok(pk) = VerifyingKey::from_bytes(public_key) {
        let sig = ed25519_dalek::Signature::from_bytes(signature);
        pk.verify(message, &sig).is_ok()
    } else {
        false
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gen_key_length() {
        let key = gen_key();
        assert_eq!(key.len(), PUBLIC_KEY_LENGTH);
    }

    #[test]
    fn test_gen_keypair_uniqueness() {
        let (pub1, priv1) = gen_keypair();
        let (pub2, priv2) = gen_keypair();
        assert_ne!(pub1, pub2);
        assert_ne!(priv1, priv2);
        assert_eq!(pub1.len(), PUBLIC_KEY_LENGTH);
        assert_eq!(priv1.len(), SECRET_KEY_LENGTH);
    }
}