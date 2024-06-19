
/// basic structure for an address
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct Address {
    pub line1: String,
    pub city: String,
    pub state: String,
    pub zip: String,
    pub zip4: Option<String>,
}

impl Address {
    pub fn full_zip(&self) -> String {
        match &self.zip4 {
            Some(zip4) => format!("{}-{}", self.zip, zip4),
            None => self.zip.clone(),
        }
    }
}

/// Complete ATMB information for a mailbox
#[derive(Debug, Hash, PartialEq, Eq)]
pub struct Mailbox {
    pub name: String,
    pub address: Address,
    pub link: String,
    pub price: String,
}