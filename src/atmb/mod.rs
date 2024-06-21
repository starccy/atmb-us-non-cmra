use anyhow::bail;
use futures::StreamExt;
use log::info;
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use crate::atmb::model::Mailbox;
use crate::atmb::page::{CountryPage, LocationDetailPage, StatePage};

mod page;
pub mod model;

const BASE_URL: &str = "https://www.anytimemailbox.com";
const UA: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0";

const US_HOME_PAGE_URL: &str = "/l/usa";

/// HTTP client for obtaining information from ATMB
struct ATMBClient {
    client: Client,
}

impl ATMBClient {
    fn new() -> anyhow::Result<Self> {
        Ok(
            Self {
                client: Client::builder()
                    .default_headers(Self::default_headers())
                    .build()?,
            }
        )
    }

    fn default_headers() -> HeaderMap {
        let mut map = HeaderMap::new();
        map.insert(USER_AGENT, HeaderValue::from_static(UA));
        map
    }

    /// get the content of a page
    ///
    /// * `url_path` - the path of the page, can be either a full URL or a relative path
    async fn fetch_page(&self, url_path: &str) -> anyhow::Result<String> {
        let url = if url_path.starts_with("http") {
            url_path
        } else {
            &format!("{}{}", BASE_URL, url_path)
        };
        Ok(
            self.client
                .get(url)
                .send()
                .await?
                .text()
                .await?
        )
    }
}

pub struct ATMBCrawl {
    client: ATMBClient,
}

impl ATMBCrawl {
    pub fn new() -> anyhow::Result<Self> {
        Ok(
            Self {
                client: ATMBClient::new()?,
            }
        )
    }

    pub async fn fetch(&self) -> anyhow::Result<Vec<Mailbox>> {
        // we're only interested in US, so hardcode here.
        let country_html = self.client.fetch_page(US_HOME_PAGE_URL).await?;
        let country_page = CountryPage::parse_html(&country_html)?;

        let state_pages = self.fetch_state_pages(&country_page).await?;
        let total_num = state_pages.iter().map(|sp| sp.len()).sum::<usize>();

        let mailboxes = state_pages.into_iter()
            .filter_map(|sp| match sp.to_mailboxes() {
                Ok(mailboxes) => Some(mailboxes),
                Err(e) => {
                    log::error!("cannot convert state page to mailboxes: {:?}", e);
                    None
                }
            })
            .flatten()
            .collect::<Vec<_>>();

        if mailboxes.len() != total_num {
            bail!("Some mailboxes cannot be fetched");
        }

        // visit every mailbox detail page to get the address line 2
        let mailboxes = self.update_street2_for_mailbox(mailboxes).await?;
        if mailboxes.len() != total_num {
            bail!("Some mailbox's detail cannot be fetched");
        }

        Ok(mailboxes)
    }

    async fn update_street2_for_mailbox(&self, mailboxes: Vec<Mailbox>) -> anyhow::Result<Vec<Mailbox>> {
        let total_mailboxes = mailboxes.len();

        let mailboxes = futures::stream::iter(mailboxes).enumerate().map(|(idx, mut mailbox)| {
            async move {
                let fut = || async {
                    info!("[{}/{}] fetching the detail page of [{}]...", idx + 1, total_mailboxes, mailbox.name);
                    let detail_page = self.fetch_location_detail_page(&mailbox.link).await?;
                    mailbox.address.line1 = detail_page.street();
                    Result::<_, anyhow::Error>::Ok(mailbox)
                };
                match fut().await {
                    Ok(mailbox) => Some(mailbox),
                    Err(err) => {
                        log::error!("cannot fetch detail page for: {:?}", err);
                        None
                    }
                }
            }
        })
            .buffer_unordered(10)
            .collect::<Vec<_>>()
            .await;

        // let mailboxes = mailboxes.into_iter().filter_map(|mailbox| match mailbox {
        //     Ok(mailbox) => Some(mailbox),
        //     Err(e) => {
        //         log::error!("cannot fetch detail page: {:?}", e);
        //         None
        //     }
        // })
        //     .collect();
        let mailboxes = mailboxes.into_iter().filter_map(|mailbox| mailbox).collect();
        Ok(mailboxes)
    }

    async fn fetch_state_pages(&self, country_page: &CountryPage<'_>) -> anyhow::Result<Vec<StatePage>> {
        let total_states = country_page.states.len();
        let state_pages: Vec<anyhow::Result<StatePage>> = futures::stream::iter(&country_page.states).enumerate().map(|(idx, state_html_info)| {
            info!("[{}/{total_states}] fetching [{}] state page...", idx + 1, state_html_info.name());
            async move {
                let state_html = self.client.fetch_page(state_html_info.url()).await?;
                Ok(StatePage::parse_html(&state_html)?)
            }
        })
            // limit concurrent requests to 5
            .buffer_unordered(5)
            .collect()
            .await;

        if state_pages.iter().filter_map(|state_page| match state_page {
            Err(e) => {
                log::error!("cannot fetch state: {:?}", e);
                Some(())
            }
            _ => None
        })
            .count() != 0 {
            bail!("Some states cannot be fetched");
        }
        Ok(state_pages.into_iter().map(|state_page| state_page.unwrap()).collect())
    }

    async fn fetch_location_detail_page(&self, mailbox_link: &str) -> anyhow::Result<LocationDetailPage> {
        let html = self.client.fetch_page(mailbox_link).await?;
        Ok(LocationDetailPage::parse_html(&html)?)
    }
}