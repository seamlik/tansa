use mockall::automock;
use uuid::Uuid;

#[automock]
pub trait IdGenerator {
    fn generate(&self) -> Vec<u8>;
}

pub struct UuidGenerator;

impl IdGenerator for UuidGenerator {
    fn generate(&self) -> Vec<u8> {
        Uuid::new_v4().into_bytes().into()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn response_id_has_reasonable_size() {
        let response_id = UuidGenerator.generate();
        assert!(response_id.len() <= 16);
    }
}
