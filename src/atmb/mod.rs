use anyhow::bail;
use futures::StreamExt;
use log::info;
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use crate::atmb::model::Mailbox;
use crate::atmb::page::{CountryPage, StatePage};

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

    async fn fetch_page(&self, sub_url_path: &str) -> anyhow::Result<String> {
        Ok(
            self.client
                .get(&format!("{}{}", BASE_URL, sub_url_path))
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
}