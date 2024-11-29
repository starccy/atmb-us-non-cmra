use std::sync::LazyLock;
use anyhow::{anyhow, bail};
use regex::Regex;
use scraper::{Html, Selector};
use crate::atmb::model::{Address, Mailbox};

static STATE_LIST_REG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"<a class='theme-simple-link' href='(.*?)'>(.*?)</a>"#).unwrap());

static LOCATION_CONTAINER_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse(r#"div[class="theme-location-item"]"#).unwrap());
static LOCATION_TITLE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse(r#"h3[class="t-title"]"#).unwrap());
static LOCATION_PRICE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse(r#"div[class="t-price"]"#).unwrap());
static LOCATION_ADDRESS_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse(r#"div[class="t-addr"]"#).unwrap());
static LOCATION_PLAN_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse(r#"a[class~="gt-plan"]"#).unwrap());
static LOCATION_DETAIL_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse(r#"div[class="t-sec1"] div[class="t-text"]"#).unwrap());

/// ATMB country page. i.e. https://www.anytimemailbox.com/l/usa
#[derive(Debug)]
pub struct CountryPage<'a> {
    pub states: Vec<StateHtmlInfo<'a>>,
}

#[derive(Debug)]
pub struct StateHtmlInfo<'a> {
    sub_url: &'a str,
    name: &'a str,
}

impl StateHtmlInfo<'_> {
    pub fn url(&self) -> &str {
        self.sub_url
    }

    pub fn name(&self) -> &str {
        self.name
    }
}

impl<'a> CountryPage<'a> {
    /// get state list from the country page
    pub fn parse_html(html: &'a str) -> anyhow::Result<Self> {
        let mut states = Vec::new();

        for caps in STATE_LIST_REG.captures_iter(html) {
            if caps.len() != 3 {
                bail!("Unexpected capture length: {}, page structure might be changed", caps.len());
            }
            states.push(StateHtmlInfo {
                sub_url: caps.get(1).unwrap().as_str(),
                name: caps.get(2).unwrap().as_str(),
            });
        }
        if states.is_empty() {
            bail!("No state found, page structure might be changed");
        }
        Ok(
            Self {
                states,
            }
        )
    }
}

/// ATMB state page. i.e. https://www.anytimemailbox.com/l/usa/alabama
pub struct StatePage {
    locations: Vec<LocationHtmlInfo>,
}

impl StatePage {
    pub fn len(&self) -> usize {
        self.locations.len()
    }
}

#[derive(Debug, Clone)]
pub struct LocationHtmlInfo {
    name: String,
    /// street address
    line1: String,
    /// city, state, zip
    line2: String,
    price: String,
    link: String,
}

impl StatePage {
    pub fn parse_html(html: &str) -> anyhow::Result<Self> {
        let mut locations = Vec::new();

        let document = Html::parse_document(html);
        let location_container = document.select(&LOCATION_CONTAINER_SELECTOR);

        for location_fragment in location_container {
            let title = location_fragment.select(&LOCATION_TITLE_SELECTOR).next()
                .ok_or_else(|| anyhow!("No title found - {}", location_fragment.html()))?
                .text()
                .collect::<String>();
            let price = location_fragment.select(&LOCATION_PRICE_SELECTOR).next()
                .ok_or_else(|| anyhow!("No price found - {}", location_fragment.html()))?
                .text()
                .collect::<String>();
            let address = location_fragment.select(&LOCATION_ADDRESS_SELECTOR).next()
                .ok_or_else(|| anyhow!("No address found - {}", location_fragment.html()))?
                .inner_html();
            let (line1, line2) = Self::split_address(&address)
                .ok_or_else(|| anyhow!("Failed to split address - {}", address))?;
            let plan_link = location_fragment.select(&LOCATION_PLAN_SELECTOR).next()
                .ok_or_else(|| anyhow!("No plan button found - {}", location_fragment.html()))?
                .value()
                .attr("href")
                .ok_or_else(|| anyhow!("No plan link found - {}", location_fragment.html()))?;

            let location_link = format!("{}{}", super::BASE_URL, plan_link);
            locations.push(LocationHtmlInfo {
                name: title,
                line1: line1.to_string(),
                line2: line2.to_string(),
                price,
                link: location_link,
            });
        }

        Ok(
            Self {
                locations,
            }
        )
    }

    pub fn to_mailboxes(&self) -> anyhow::Result<Vec<Mailbox>> {
        self.locations.iter()
            .map(|location| location.clone().try_into())
            .collect()
    }

    fn split_address(address: &str) -> Option<(&str, &str)> {
        let mut segments = address.split("<br>").take(2);
        Some((segments.next()?, segments.next()?))
    }
}

impl LocationHtmlInfo {
    fn parse_city(&self) -> Option<&str> {
        self.line2.split(",")
            .next()
    }

    fn parse_state(&self) -> Option<&str> {
        self.line2.split(",")
            .skip(1)
            .next()
            .map(|s| s.trim())
            .and_then(|s| s.split(" ").next())
    }

    fn parse_zip(&self) -> Option<(&str, Option<&str>)> {
        fn try_split_zip(zip_str: &str) -> Option<(&str, Option<&str>)> {
            let mut segments = zip_str.split("-");
            let zip = segments.next()?;
            let zip4 = segments.next();
            Some((zip, zip4))
        }

        self.line2.split(",")
            .skip(1)
            .next()
            .map(|s| s.trim())
            .and_then(|s| s.split(" ").skip(1).next())
            .and_then(|s| try_split_zip(s))
    }

    fn price(&self) -> String {
        self.price.replace("Starting from", "")
            .replace(" ", "")
    }
}

/// ATMB location detail page. i.e. https://www.anytimemailbox.com/s/birmingham-120-19th-street-north
pub struct LocationDetailPage {
    /// street address
    line1: String,
    /// unit, suite, etc.
    line2: Option<String>,
}

impl LocationDetailPage {
    pub fn parse_html(html: &str) -> anyhow::Result<Self> {
        let document = Html::parse_document(html);
        let address_container = document.select(&LOCATION_DETAIL_SELECTOR).next().unwrap();
        let div_selector = Selector::parse("div").unwrap();

        let lines = address_container.select(&div_selector)
            .map(|div| div.text().collect::<String>())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();

        let line2 = match lines.len() {
            // no line2
            4 => None,
            // one-line line2
            5 => Some(lines[2].to_string()),
            // two-line line2
            6 => Some(format!("{} {}", lines[2], lines[3])),
            _ => bail!("Unexpected address line count: {}, page structure might be changed: {:?}", lines.len(), lines),
        };
        Ok(
            Self {
                line1: lines[1].clone(),
                line2,
            }
        )
    }

    /// concatenate line1 and line2
    pub fn street(&self) -> String {
        if let Some(line2) = &self.line2 {
            format!("{} {}", self.line1, line2)
        } else {
            self.line1.clone()
        }
    }
}

impl TryInto<Address> for LocationHtmlInfo {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<Address, Self::Error> {
        let (zip, zip4) = self.parse_zip().ok_or_else(|| anyhow!("Failed to parse zip code from: {}", self.line2))?;
        Ok(
            Address {
                city: self.parse_city().ok_or_else(|| anyhow!("Failed to parse city from: {}", self.line2))?.to_string(),
                state: self.parse_state().ok_or_else(|| anyhow!("Failed to parse state from: {}", self.line2))?.to_string(),
                zip: zip.to_owned(),
                zip4: zip4.map(|s| s.to_owned()),
                line1: self.line1,
            }
        )
    }
}

impl TryInto<Mailbox> for LocationHtmlInfo {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<Mailbox, Self::Error> {
        Ok(
            Mailbox {
                address: self.clone().try_into()?,
                price: self.price(),
                name: self.name,
                link: self.link,
            }
        )
    }
}
