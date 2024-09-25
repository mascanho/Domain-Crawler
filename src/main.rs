use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use std::collections::{HashSet, VecDeque};
use std::time::Duration;
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let start_url = "https://www.algarvewonders.com/";
    let mut crawler = Crawler::new(start_url);
    let results = crawler.crawl().await?;
    println!("Crawl results: {:?}", results);
    Ok(())
}

struct Crawler {
    client: Client,
    to_visit: VecDeque<String>,
    visited: HashSet<String>,
    link_regex: Regex,
    base_domain: String,
}

impl Crawler {
    fn new(start_url: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        let link_regex = Regex::new(r#"(?i)(?:href|src)=["']([^"']+)["']"#).unwrap();
        let base_domain = Url::parse(start_url)
            .expect("Invalid start URL")
            .domain()
            .expect("Unable to extract domain")
            .to_string();

        Crawler {
            client,
            to_visit: VecDeque::from([start_url.to_string()]),
            visited: HashSet::new(),
            link_regex,
            base_domain,
        }
    }

    async fn crawl(&mut self) -> Result<Vec<CrawlResult>, Box<dyn std::error::Error>> {
        let mut results = Vec::new();

        while let Some(url) = self.to_visit.pop_front() {
            if self.visited.contains(&url) {
                continue;
            }

            println!("Crawling: {}", url);
            self.visited.insert(url.clone());

            match self.client.get(&url).send().await {
                Ok(response) => {
                    let content_type = response
                        .headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();

                    if content_type.starts_with("text/html") {
                        let body = response.text().await?;
                        let links = self.parse_html(&url, &body)?;
                        for link in &links {
                            if !self.visited.contains(link) && self.is_same_domain(link) {
                                self.to_visit.push_back(link.to_string());
                            }
                        }
                        results.push(CrawlResult::Html { url, links });
                    } else {
                        let bytes = response.bytes().await?;
                        results.push(CrawlResult::File {
                            url,
                            content_type,
                            content: bytes.to_vec(),
                        });
                    }
                }
                Err(e) => {
                    results.push(CrawlResult::Error {
                        url,
                        error: e.to_string(),
                    });
                }
            }

            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        Ok(results)
    }

    fn parse_html(
        &mut self,
        base_url: &str,
        html: &str,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut links = Vec::new();
        links.extend(self.parse_html_with_scraper(base_url, html)?);
        links.extend(self.parse_html_with_regex(base_url, html)?);
        Ok(links)
    }

    fn parse_html_with_scraper(
        &mut self,
        base_url: &str,
        html: &str,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let document = Html::parse_document(html);
        let selector = Selector::parse("a, link, script, img, source").unwrap();
        let mut links = Vec::new();

        for element in document.select(&selector) {
            let href = element
                .value()
                .attr("href")
                .or_else(|| element.value().attr("src"));

            if let Some(href) = href {
                if let Some(url) = self.normalize_url(base_url, href) {
                    links.push(url);
                }
            }
        }

        Ok(links)
    }

    fn parse_html_with_regex(
        &mut self,
        base_url: &str,
        html: &str,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let captures: Vec<_> = self.link_regex.captures_iter(html).collect();
        let mut links = Vec::new();

        for cap in captures {
            if let Some(href) = cap.get(1) {
                if let Some(url) = self.normalize_url(base_url, href.as_str()) {
                    links.push(url);
                }
            }
        }

        Ok(links)
    }

    fn normalize_url(&self, base_url: &str, href: &str) -> Option<String> {
        if let Ok(absolute_url) =
            Url::parse(href).or_else(|_| Url::parse(base_url).and_then(|base| base.join(href)))
        {
            Some(absolute_url.to_string())
        } else {
            None
        }
    }

    fn is_same_domain(&self, url: &str) -> bool {
        if let Ok(parsed_url) = Url::parse(url) {
            parsed_url.domain() == Some(&self.base_domain)
        } else {
            false
        }
    }
}

#[derive(Debug)]
enum CrawlResult {
    Html {
        url: String,
        links: Vec<String>,
    },
    File {
        url: String,
        content_type: String,
        content: Vec<u8>,
    },
    Error {
        url: String,
        error: String,
    },
}
