use serde::Serialize;
use crate::atmb::model::Mailbox;
use crate::smarty::{AdditionalInfo, Rdi, YesOrNo};

/// The final struct that will be used to store the data
#[derive(Debug, Serialize)]
pub struct Record {
    name: String,
    street: String,
    city: String,
    state: String,
    zip: String,
    price: String,
    link: String,
    pub rdi: Rdi,
    #[serde(rename = "CMRA")]
    pub cmra: YesOrNo,
}

impl Record {
    pub fn from_mailbox_and_info(mailbox: Mailbox, info: AdditionalInfo) -> Self {
        Self {
            zip: mailbox.address.full_zip(),
            name: mailbox.name,
            street: mailbox.address.line1,
            city: mailbox.address.city,
            state: mailbox.address.state,
            price: mailbox.price,
            link: mailbox.link,
            rdi: info.rdi,
            cmra: info.cmra,
        }
    }
}
