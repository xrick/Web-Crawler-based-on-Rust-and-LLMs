//! Limited Apple downloader. Redirects are checked before following them.
use crate::apple::allowed;
use regex::Regex;
use reqwest::{Client, StatusCode};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use url::Url;

type RobotGroup = (Vec<String>, Vec<(bool, String)>);
#[derive(Default)]
pub struct Robots {
    rules: Vec<(bool, String)>,
}
impl Robots {
    pub fn parse(body: &str) -> Self {
        let mut groups: Vec<RobotGroup> = vec![];
        let mut agents = vec![];
        let mut rules = vec![];
        let mut saw_rule = false;
        for line in body.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            let Some((key, val)) = line.split_once(':') else {
                continue;
            };
            let val = val.trim();
            match key.trim().to_lowercase().as_str() {
                "user-agent" => {
                    if saw_rule {
                        groups.push((std::mem::take(&mut agents), std::mem::take(&mut rules)));
                        saw_rule = false;
                    }
                    agents.push(val.to_lowercase());
                }
                "allow" | "disallow" if !agents.is_empty() => {
                    saw_rule = true;
                    if !val.is_empty() {
                        rules.push((key.trim().eq_ignore_ascii_case("allow"), val.into()));
                    }
                }
                _ => {}
            }
        }
        groups.push((agents, rules));
        let specific = groups
            .iter()
            .any(|(a, _)| a.iter().any(|v| v == "rustcrawler"));
        Self {
            rules: groups
                .into_iter()
                .filter(|(a, _)| {
                    a.iter().any(|v| {
                        if specific {
                            v == "rustcrawler"
                        } else {
                            v == "*"
                        }
                    })
                })
                .flat_map(|(_, r)| r)
                .collect(),
        }
    }
    pub fn permits(&self, path: &str) -> bool {
        let mut decision = (0, true);
        for (allow, pattern) in &self.rules {
            let end = pattern.ends_with('$');
            let pattern = pattern.trim_end_matches('$');
            let regex = format!(
                "^{}{}",
                pattern
                    .split('*')
                    .map(regex::escape)
                    .collect::<Vec<_>>()
                    .join(".*"),
                if end { "$" } else { "" }
            );
            if Regex::new(&regex).is_ok_and(|r| r.is_match(path)) {
                let n = pattern.replace('*', "").len();
                if n > decision.0 || (n == decision.0 && *allow) {
                    decision = (n, *allow);
                }
            }
        }
        decision.1
    }
}
pub struct Downloader {
    client: Client,
    pub robots: Robots,
    cancel: CancellationToken,
}
impl Downloader {
    pub fn new(cancel: CancellationToken) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("RustCrawler/0.1 (local product research)")
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            client,
            robots: Robots::default(),
            cancel,
        })
    }
    pub async fn load_robots(&mut self) -> Result<String, String> {
        let (_, body) = self.get("https://www.apple.com/robots.txt").await?;
        self.robots = Robots::parse(&body);
        Ok(body)
    }
    pub async fn get(&self, initial: &str) -> Result<(String, String), String> {
        let mut url = Url::parse(initial).map_err(|e| e.to_string())?;
        for _ in 0..6 {
            if !allowed(&url) {
                return Err(format!("拒絕非 Apple 台灣 URL: {url}"));
            }
            if !self.robots.permits(url.path()) {
                return Err("robots.txt 不允許此路徑".into());
            }
            let mut response = None;
            for attempt in 0..3 {
                tokio::select! { _ = self.cancel.cancelled() => return Err("cancelled".into()), _ = tokio::time::sleep(Duration::from_secs(1 << attempt)) => {} }
                let result = tokio::select! { _ = self.cancel.cancelled() => return Err("cancelled".into()), r = self.client.get(url.clone()).send() => r };
                match result {
                    Ok(r)
                        if (r.status().is_server_error()
                            || r.status() == StatusCode::TOO_MANY_REQUESTS)
                            && attempt < 2 =>
                    {
                        continue;
                    }
                    Ok(r) => {
                        response = Some(r);
                        break;
                    }
                    Err(_) if attempt < 2 => continue,
                    Err(e) => return Err(e.to_string()),
                }
            }
            let mut r = response.ok_or("download failed")?;
            if r.status().is_redirection() {
                let location = r
                    .headers()
                    .get("location")
                    .ok_or("redirect without location")?
                    .to_str()
                    .map_err(|e| e.to_string())?;
                url = url.join(location).map_err(|e| e.to_string())?;
                continue;
            }
            if !r.status().is_success() {
                return Err(format!("HTTP {}", r.status()));
            }
            let ct = r
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if url.path() != "/robots.txt" && !ct.contains("text/html") {
                return Err(format!("不支援內容類型 {ct}"));
            }
            let mut bytes = vec![];
            loop {
                let chunk = tokio::select! { _ = self.cancel.cancelled() => return Err("cancelled".into()), c = r.chunk() => c.map_err(|e|e.to_string())? };
                let Some(chunk) = chunk else {
                    break;
                };
                if bytes.len() + chunk.len() > 8_000_000 {
                    return Err("網頁超過 8 MB 上限".into());
                }
                bytes.extend_from_slice(&chunk);
            }
            return String::from_utf8(bytes)
                .map(|b| (url.to_string(), b))
                .map_err(|e| e.to_string());
        }
        Err("重新導向次數超過上限".into())
    }
}
