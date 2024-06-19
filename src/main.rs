use std::collections::HashMap;
use std::path::Path;
use futures::StreamExt;
use log::{error, info};
use crate::atmb::ATMBCrawl;
use crate::atmb::model::Mailbox;
use crate::record::Record;
use crate::smarty::{AdditionalInfo, SmartyClientProxy};

mod atmb;
mod record;
mod smarty;

#[tokio::main]
async fn main() {
    env_logger::init();

    match run().await {
        Err(e) => {
            log::error!("Error: {:?}", e);
            std::process::exit(1);
        }
        _ => {}
    }
}

async fn run() -> anyhow::Result<()> {
    let atmb = ATMBCrawl::new()?;
    let mailboxes = atmb.fetch().await?;

    info!("finished fetching, got [{}] mailboxes in total", mailboxes.len());
    info!("begin to inquire mailbox address info...");

    let mailboxes_info = inquire_mailboxes_info(mailboxes).await?;
    // filter out CMRA and addresses
    let records = mailboxes_info.into_iter().filter_map(|(mailbox, info)| {
        if info.is_cmra() {
            None
        } else {
            Some(Record::from_mailbox_and_info(mailbox, info))
        }
    })
        .collect::<Vec<_>>();

    let out_file = "result/mailboxes.csv";
    info!("saving records to [{}]", out_file);
    save_records(records, out_file)?;
    Ok(())
}

async fn inquire_mailboxes_info(mailboxes: Vec<Mailbox>) -> anyhow::Result<HashMap<Mailbox, AdditionalInfo>> {
    let client = SmartyClientProxy::new()?;

    let total = mailboxes.len();
    let mailboxes_info = futures::stream::iter(mailboxes.into_iter()).enumerate().map(|(idx, mailbox)| {
        let client = &client;
        async move {
            info!("[{}/{total}] fetching mailbox address info for [{}]", idx + 1, mailbox.name);

            let address = &mailbox.address;
            let additional_info = match client.inquire_address(address.clone()).await {
                Ok(info) => info,
                Err(e) => {
                    error!("cannot inquire address info for [{}]: {:?}", mailbox.name, e);
                    return None;
                }
            };
            Some((mailbox, additional_info))
        }
    })
        .buffer_unordered(10)
        .collect::<Vec<_>>()
        .await;

    Ok(mailboxes_info.into_iter().filter_map(|info| info).collect::<HashMap<_, _>>())
}

/// write result to CSV file
fn save_records(mut records: Vec<Record>, save_path: impl AsRef<Path>) -> anyhow::Result<()> {
    records.sort_by(|r1, r2| (&r1.cmra, &r1.rdi).cmp(&(&r2.cmra, &r2.rdi)));
    if let Some(parent) = save_path.as_ref().parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut wtr = csv::Writer::from_path(save_path)?;
    for record in &records {
        wtr.serialize(record)?;
    }
    Ok(())
}
