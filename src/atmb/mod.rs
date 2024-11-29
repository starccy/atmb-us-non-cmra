use color_eyre::eyre::{bail, eyre};
use futures::StreamExt;
use log::info;
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use crate::atmb::model::Mailbox;
use crate::atmb::page::{CountryPage, LocationDetailPage, StatePage};
use crate::utils::retry_wrapper;

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
    fn new() -> color_eyre::Result<Self> {
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
    async fn fetch_page(&self, url_path: &str) -> color_eyre::Result<String> {
        let url = if url_path.starts_with("http") {
            url_path
        } else {
            &format!("{}{}", BASE_URL, url_path)
        };
        Ok(
            retry_wrapper(3, || async {
                self.client
                    .get(url)
                    .send()
                    .await?
                    .text()
                    .await
            }).await?
        )
    }
}

pub struct ATMBCrawl {
    client: ATMBClient,
}

impl ATMBCrawl {
    pub fn new() -> color_eyre::Result<Self> {
        Ok(
            Self {
                client: ATMBClient::new()?,
            }
        )
    }

    pub async fn fetch(&self) -> color_eyre::Result<Vec<Mailbox>> {
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
        self.update_street2_for_mailbox(mailboxes).await.map_err(|e| {
            eyre!("Some mailbox's detail cannot be fetched: {:?}", e)
        })
    }

    async fn update_street2_for_mailbox(&self, mailboxes: Vec<Mailbox>) -> color_eyre::Result<Vec<Mailbox>> {
        let total_mailboxes = mailboxes.len();

        let mailboxes = futures::stream::iter(mailboxes).enumerate().map(|(idx, mut mailbox)| {
            let link = mailbox.link.clone();
            async move {
                let fut = || async {
                    info!("[{}/{}] fetching the detail page of [{}]...", idx + 1, total_mailboxes, mailbox.name);
                    let detail_page = self.fetch_location_detail_page(&mailbox.link).await?;
                    mailbox.address.line1 = detail_page.street();
                    Result::<_, color_eyre::eyre::Error>::Ok(mailbox)
                };
                fut().await
                    .map_err(|err| {
                        let err = eyre!("cannot fetch detail page for: [{}]: {:?}", link, err);
                        log::error!("{:?}", err);
                        err
                    })
            }
        })
            .buffer_unordered(10)
            .collect::<Vec<_>>()
            .await;

        let (suc_list, err_list): (Vec<_>, Vec<_>) = mailboxes.into_iter().partition(Result::is_ok);
        let suc_list = suc_list.into_iter().filter_map(Result::ok).collect::<Vec<_>>();
        let err_list = err_list.into_iter().filter_map(Result::err).collect::<Vec<_>>();

        if !err_list.is_empty() {
            bail!("{:#?}", err_list);
        } else {
            Ok(suc_list)
        }
    }

    async fn fetch_state_pages(&self, country_page: &CountryPage<'_>) -> color_eyre::Result<Vec<StatePage>> {
        let total_states = country_page.states.len();
        let state_pages: Vec<color_eyre::Result<StatePage>> = futures::stream::iter(&country_page.states).enumerate().map(|(idx, state_html_info)| {
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

    async fn fetch_location_detail_page(&self, mailbox_link: &str) -> color_eyre::Result<LocationDetailPage> {
        let html = self.client.fetch_page(mailbox_link).await?;
        Ok(LocationDetailPage::parse_html(&html)?)
    }
}