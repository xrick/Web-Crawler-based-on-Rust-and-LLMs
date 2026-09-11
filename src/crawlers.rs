//! Capability hierarchy: discovery and extraction combine into a product crawler.
//! Add a site implementation here; job scheduling and persistence remain in Engine.
use crate::{
    apple,
    models::{Block, Candidate, Price},
};

pub trait Discovery: Send + Sync {
    fn entry_url(&self, category: &str) -> String;
    fn candidates(&self, html: &str, url: &str, category: &str) -> Vec<String>;
    fn resolve(&self, html: &str, candidate: &mut Candidate);
}
pub trait Extraction: Send + Sync {
    fn blocks(&self, html: &str) -> Vec<Block>;
    fn name(&self, html: &str) -> String;
    fn price(&self, html: &str, url: &str) -> Option<Price>;
    fn plain(&self, html: &str) -> String;
}
pub trait Crawler: Discovery + Extraction {
    fn robots_url(&self) -> &str;
}

pub struct AppleTaiwanCrawler;
impl Discovery for AppleTaiwanCrawler {
    fn entry_url(&self, category: &str) -> String {
        format!("https://www.apple.com/tw/{category}/")
    }
    fn candidates(&self, html: &str, url: &str, category: &str) -> Vec<String> {
        apple::links(html, url)
            .into_iter()
            .filter(|u| apple::category_for(u) == Some(category))
            .collect()
    }
    fn resolve(&self, html: &str, candidate: &mut Candidate) {
        let prefix = format!("{}/", candidate.url.trim_end_matches('/'));
        let links = apple::links(html, &candidate.url);
        candidate.specs_url = links
            .iter()
            .find(|s| s.starts_with(&prefix) && (s.ends_with("/specs/") || s.ends_with("/specs")))
            .or_else(|| {
                links
                    .iter()
                    .find(|s| s.starts_with(&prefix) && s.contains("tech-specs"))
            })
            .cloned();
        candidate.buy_url = apple::buy_link(html, &candidate.url);
    }
}
impl Extraction for AppleTaiwanCrawler {
    fn blocks(&self, html: &str) -> Vec<Block> {
        apple::blocks(html)
    }
    fn name(&self, html: &str) -> String {
        apple::product_name(html)
    }
    fn price(&self, html: &str, url: &str) -> Option<Price> {
        apple::price(html, url)
    }
    fn plain(&self, html: &str) -> String {
        apple::plain(html)
    }
}
impl Crawler for AppleTaiwanCrawler {
    fn robots_url(&self) -> &str {
        "https://www.apple.com/robots.txt"
    }
}
