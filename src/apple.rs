//! Apple-specific link discovery and lossless section extraction, independent of the LLM.
use crate::models::{Block, Price};
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;
use url::Url;

pub fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("static CSS selector")
}
pub fn clean(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
pub fn text(node: ElementRef<'_>) -> String {
    // Keep block boundaries; omit executable code and duplicated noscript content.
    let mut output = String::new();
    for child in node.descendants() {
        if child
            .ancestors()
            .filter_map(ElementRef::wrap)
            .any(|e| matches!(e.value().name(), "script" | "style" | "noscript" | "sup"))
        {
            continue;
        }
        if let Some(t) = child.value().as_text() {
            output.push_str(t);
        }
        if let Some(e) = ElementRef::wrap(child)
            && matches!(
                e.value().name(),
                "li" | "p" | "br" | "div" | "h2" | "h3" | "h4" | "tr"
            )
        {
            output.push('\n');
        }
    }
    output
        .lines()
        .map(clean)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
pub fn allowed(url: &Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("www.apple.com")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && (url.path().starts_with("/tw/") || url.path() == "/robots.txt")
}
pub fn canonical(base: &str, href: &str) -> Option<String> {
    let mut url = Url::parse(base).ok()?.join(href).ok()?;
    if !allowed(&url) {
        return None;
    }
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string())
}
pub fn category_for(url: &str) -> Option<&'static str> {
    let parsed = Url::parse(url).ok()?;
    let segments: Vec<_> = parsed.path().trim_matches('/').split('/').collect();
    if segments.len() != 2 || segments[0] != "tw" {
        return None;
    }
    let s = segments[1];
    if s.starts_with("iphone-") {
        Some("iphone")
    } else if s.starts_with("macbook-")
        || matches!(s, "imac" | "mac-mini" | "mac-studio" | "mac-pro")
    {
        Some("mac")
    } else if s.starts_with("ipad-") && !s.contains("keyboard") {
        Some("ipad")
    } else if s.starts_with("apple-watch-")
        && !s.contains("hermes")
        && !s.contains("nike")
        && !s.contains("for-your-kids")
    {
        Some("watch")
    } else if s.starts_with("airpods-") && !s.contains("compare") {
        Some("airpods")
    } else {
        None
    }
}
pub fn links(html: &str, base: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let mut links: Vec<_> = doc
        .select(&sel("a[href]"))
        .filter_map(|a| canonical(base, a.value().attr("href")?))
        .collect();
    links.sort();
    links.dedup();
    links
}
pub fn product_name(html: &str) -> String {
    let doc = Html::parse_document(html);
    doc.select(&sel("h1"))
        .next()
        .or_else(|| doc.select(&sel("title")).next())
        .map(text)
        .unwrap_or_default()
        .replace("技術規格", "")
        .replace("- Apple (台灣)", "")
        .trim()
        .trim_end_matches('-')
        .trim()
        .to_owned()
}
pub fn plain(html: &str) -> String {
    let doc = Html::parse_document(html);
    doc.select(&sel("main"))
        .next()
        .or_else(|| doc.select(&sel("body")).next())
        .map(text)
        .unwrap_or_default()
}
pub fn blocks(html: &str) -> Vec<Block> {
    let doc = Html::parse_document(html);
    let mut out = vec![];
    for row in doc.select(&sel(".techspecs-row, tr")) {
        if row
            .ancestors()
            .filter_map(ElementRef::wrap)
            .any(|e| e.value().classes().any(|c| c == "techspecs-header-row"))
        {
            continue;
        }
        let section = row
            .select(&sel(".techspecs-rowheader, [role=rowheader], th"))
            .next()
            .map(text)
            .unwrap_or_else(|| "其他規格".into());
        let table = row.ancestors().filter_map(ElementRef::wrap).find(|e| {
            e.value()
                .attr("role")
                .is_some_and(|r| r.split_whitespace().any(|r| r == "table"))
                || e.value().name() == "table"
                || e.value().classes().any(|c| c == "techspecs")
        });
        let headers: Vec<_> = table
            .map(|t| {
                t.select(&sel("[role=columnheader], .techspecs-header-row h2"))
                    .map(text)
                    .filter(|s| !s.trim().is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let context = row
            .ancestors()
            .filter_map(ElementRef::wrap)
            .find(|e| e.value().attr("role") == Some("tabpanel"))
            .and_then(|e| e.value().attr("aria-labelledby"))
            .unwrap_or("")
            .to_string();
        let cells: Vec<_> = row.select(&sel(".techspecs-column, td")).collect();
        let count = cells.len();
        for (i, cell) in cells.into_iter().enumerate() {
            let value = text(cell);
            if value.is_empty() {
                continue;
            }
            // A shared colspan stays unassigned: don't guess that it applies to every model.
            let hint = if headers.len() == count
                && cell.value().attr("aria-colspan").unwrap_or("1") == "1"
            {
                headers
                    .get(i)
                    .map(|s| format!("{} {}", context, s).trim().to_string())
            } else {
                None
            };
            out.push(Block {
                id: out.len(),
                section: section.clone(),
                model_hint: hint,
                context: context.clone(),
                text: value,
            });
        }
    }
    if out.is_empty() {
        for section in doc.select(&sel("main section")) {
            // Keep only leaf sections, avoiding duplicate nested content.
            if section.select(&sel("section")).next().is_some() {
                continue;
            }
            let value = text(section);
            if value.is_empty() {
                continue;
            }
            let title = section
                .select(&sel("h2, h3"))
                .next()
                .map(text)
                .unwrap_or_else(|| "待確認段落".into());
            if clean(&value) == clean(&title) {
                continue;
            }
            let grid_cells: Vec<_> = section
                .select(&sel(".grid-container > .grid-item"))
                .collect();
            let model_pattern =
                regex::Regex::new(r"[0-9]+ 個連接埠機型(?: [0-9]+)?").expect("static pattern");
            if grid_cells.len() > 1 && grid_cells.iter().any(|c| model_pattern.is_match(&text(*c)))
            {
                for cell in grid_cells {
                    let value = text(cell);
                    if value.is_empty() {
                        continue;
                    }
                    let hints: std::collections::BTreeSet<_> = model_pattern
                        .find_iter(&value)
                        .map(|m| m.as_str().to_string())
                        .collect();
                    out.push(Block {
                        id: out.len(),
                        section: title.clone(),
                        model_hint: if hints.len() == 1 {
                            hints.into_iter().next()
                        } else {
                            None
                        },
                        context: "source-labelled-grid".into(),
                        text: value,
                    });
                }
                continue;
            }
            out.push(Block {
                id: out.len(),
                section: title,
                model_hint: None,
                context: "fallback-section".into(),
                text: value,
            });
        }
    }
    // Footnotes can qualify battery, capacity, and weight claims; preserve them separately.
    for footnote in doc.select(&sel(".ac-gf-sosumi li")) {
        let value = text(footnote);
        if !value.is_empty() {
            out.push(Block {
                id: out.len(),
                section: "註腳與限制".into(),
                model_hint: None,
                context: footnote.value().attr("id").unwrap_or("footnote").into(),
                text: value,
            });
        }
    }
    // Long cells are split without dropping any source text. IDs remain unique.
    let mut split = vec![];
    for block in out {
        let chars: Vec<_> = block.text.chars().collect();
        for part in chars.chunks(3500) {
            let mut b = block.clone();
            b.id = split.len();
            b.text = part.iter().collect();
            split.push(b);
        }
    }
    split
}

pub fn price(html: &str, url: &str) -> Option<Price> {
    let doc = Html::parse_document(html);
    for script in doc.select(&sel("script[type='application/ld+json']")) {
        let raw = script.inner_html();
        if let Ok(value) = serde_json::from_str::<Value>(&raw)
            && let Some(p) = product_price(&value, url)
        {
            return Some(p);
        }
    }
    None
}
fn product_price(value: &Value, url: &str) -> Option<Price> {
    if let Some(a) = value.as_array() {
        return a.iter().find_map(|v| product_price(v, url));
    }
    if let Some(a) = value.get("@graph") {
        return product_price(a, url);
    }
    let is_product = value["@type"] == "Product"
        || value["@type"]
            .as_array()
            .is_some_and(|a| a.iter().any(|t| t == "Product"));
    if !is_product {
        return None;
    }
    let offers = &value["offers"];
    let list = offers
        .as_array()
        .cloned()
        .unwrap_or_else(|| vec![offers.clone()]);
    list.iter()
        .filter_map(|offer| {
            if offer["priceCurrency"] != "TWD" {
                return None;
            }
            let n = offer.get("lowPrice").or_else(|| offer.get("price"))?;
            let amount = n
                .as_f64()
                .or_else(|| n.as_str()?.replace(',', "").parse().ok())?;
            if !amount.is_finite() || amount <= 0.0 {
                return None;
            }
            Some(Price {
                amount,
                currency: "TWD".into(),
                source_url: url.into(),
                source_name: value["name"].as_str().unwrap_or("").into(),
                evidence: offer.to_string(),
                scope: "官網此商品頁起售價；不代表每個選配型號的售價".into(),
            })
        })
        .min_by(|a, b| a.amount.total_cmp(&b.amount))
}

// The local navigation CTA belongs to this product even when the purchase slug differs.
pub fn buy_link(html: &str, base: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    doc.select(&sel("a.ac-ln-button[href]"))
        .filter_map(|a| canonical(base, a.value().attr("href")?))
        .find(|s| s.contains("/tw/shop/") && (s.contains("/buy-") || s.contains("/goto/buy_")))
}
