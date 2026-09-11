//! Background orchestration. Durable snapshots are saved after each meaningful step.
use crate::{
    crawlers::{AppleTaiwanCrawler, Crawler},
    llm,
    models::*,
    services::{Annotator, AppleDownloads, DownloadFactory, Fetcher, Ollama},
    storage::Store,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;

pub struct Engine {
    pub store: Store,
    crawler: Arc<dyn Crawler>,
    downloads: Arc<dyn DownloadFactory>,
    annotator: Arc<dyn Annotator>,
    active: Mutex<Option<(String, CancellationToken)>>,
}
impl Engine {
    pub fn new(store: Store) -> Self {
        Self::with_components(
            store,
            Arc::new(AppleTaiwanCrawler),
            Arc::new(AppleDownloads),
            Arc::new(Ollama),
        )
    }
    pub fn with_components(
        store: Store,
        crawler: Arc<dyn Crawler>,
        downloads: Arc<dyn DownloadFactory>,
        annotator: Arc<dyn Annotator>,
    ) -> Self {
        Self {
            store,
            crawler,
            downloads,
            annotator,
            active: Mutex::new(None),
        }
    }
    pub fn start(self: &Arc<Self>, options: RunOptions) -> Result<Job, String> {
        if options.products_per_category > 30 {
            return Err("每類產品上限需小於等於 30".into());
        }
        let mut active = self.active.lock().map_err(|e| e.to_string())?;
        if active.is_some() {
            return Err("已有工作執行中".into());
        }
        let settings = self.store.settings()?;
        settings.validate()?;
        let job = Job::new(settings, options);
        self.store.save(&job)?;
        let cancel = CancellationToken::new();
        *active = Some((job.id.clone(), cancel.clone()));
        let engine = self.clone();
        let snapshot = job.clone();
        actix_web::rt::spawn(async move {
            engine.run(snapshot, cancel).await;
        });
        Ok(job)
    }
    pub fn cancel(&self, id: &str) -> Result<(), String> {
        let active = self.active.lock().map_err(|e| e.to_string())?;
        if let Some((current, token)) = active.as_ref()
            && current == id
        {
            token.cancel();
            return Ok(());
        }
        Err("此工作目前未執行".into())
    }
    pub fn active_id(&self) -> Option<String> {
        self.active.lock().ok()?.as_ref().map(|(id, _)| id.clone())
    }
    async fn run(self: Arc<Self>, mut job: Job, cancel: CancellationToken) {
        let result = self.work(&mut job, &cancel).await;
        if cancel.is_cancelled() {
            job.status = "cancelled".into();
            job.phase = "已取消，保留完成資料".into();
        } else if let Err(e) = result {
            job.issue("", "工作", e);
            job.status = "failed".into();
            job.phase = "工作失敗".into();
        } else {
            job.status = if job.issues.is_empty() {
                "completed"
            } else {
                "completed_with_errors"
            }
            .into();
            job.phase = "完成".into();
        }
        job.finished_at = Some(now());
        if let Err(e) = self.store.save(&job) {
            eprintln!("儲存工作失敗: {e}");
        }
        if let Ok(mut active) = self.active.lock() {
            *active = None;
        }
    }
    async fn fetch(
        &self,
        job: &mut Job,
        net: &dyn Fetcher,
        url: &str,
        kind: &str,
        category: &str,
        cache: &mut HashMap<String, String>,
    ) -> Result<String, String> {
        if let Some(html) = cache.get(url) {
            return Ok(html.clone());
        }
        if job.pages.len() >= job.settings.max_pages {
            return Err("已達頁數上限；此 URL 未下載".into());
        }
        job.current_url = url.into();
        self.store.save(job)?;
        let (final_url, html) = net.get(url).await?;
        let dir = self.store.root.join(category).join(&job.id);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let n = job.pages.len();
        let html_name = format!("{category}/{}/page-{n}.html", job.id);
        let text_name = format!("{category}/{}/page-{n}.txt", job.id);
        std::fs::write(self.store.root.join(&html_name), &html).map_err(|e| e.to_string())?;
        std::fs::write(self.store.root.join(&text_name), self.crawler.plain(&html))
            .map_err(|e| e.to_string())?;
        job.pages.push(PageRecord {
            url: final_url.clone(),
            kind: kind.into(),
            category: category.into(),
            fetched_at: now(),
            html_file: html_name,
            text_file: text_name,
            sha256: format!("{:x}", Sha256::digest(html.as_bytes())),
        });
        cache.insert(url.into(), html.clone());
        cache.insert(final_url, html.clone());
        self.store.save(job)?;
        Ok(html)
    }
    async fn work(&self, job: &mut Job, cancel: &CancellationToken) -> Result<(), String> {
        let mut net = self.downloads.create(cancel.clone())?;
        let robots = net.robots(self.crawler.robots_url()).await?;
        let dir = self.store.root.join("runs").join(&job.id);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("robots.txt"), robots).map_err(|e| e.to_string())?;
        if !job.options.discovery_only {
            let models = tokio::select! {
                _ = cancel.cancelled() => return Err("cancelled".into()),
                result = self.annotator.models() => result?,
            };
            if !models.contains(&job.settings.model) {
                return Err("設定模型不在 Ollama 已安裝清單中".into());
            }
        }
        let mut cache = HashMap::new();
        job.phase = "探索產品入口".into();
        self.store.save(job)?;
        let mut seen = HashSet::new();
        for category in job.settings.categories.clone() {
            if cancel.is_cancelled() {
                return Err("cancelled".into());
            }
            let url = self.crawler.entry_url(&category);
            match self
                .fetch(job, net.as_ref(), &url, "category", &category, &mut cache)
                .await
            {
                Ok(html) => {
                    for link in self.crawler.candidates(&html, &url, &category) {
                        if seen.insert(link.clone()) {
                            job.candidates.push(Candidate {
                                category: category.clone(),
                                url: link,
                                specs_url: None,
                                buy_url: None,
                            });
                        }
                    }
                }
                Err(e) => job.issue(&url, "探索", e),
            }
        }
        // Round-robin by category prevents one category consuming the whole page budget first.
        let mut groups: BTreeMap<String, Vec<Candidate>> = BTreeMap::new();
        for c in std::mem::take(&mut job.candidates) {
            groups.entry(c.category.clone()).or_default().push(c);
        }
        while groups.values().any(|v| !v.is_empty()) {
            for category in &job.settings.categories {
                if let Some(group) = groups.get_mut(category)
                    && !group.is_empty()
                {
                    job.candidates.push(group.remove(0));
                }
            }
        }
        job.discovered = job.candidates.len();
        self.store.save(job)?;
        for i in 0..job.candidates.len() {
            if cancel.is_cancelled() {
                return Err("cancelled".into());
            }
            let c = job.candidates[i].clone();
            match self
                .fetch(
                    job,
                    net.as_ref(),
                    &c.url,
                    "product",
                    &c.category,
                    &mut cache,
                )
                .await
            {
                Ok(html) => {
                    self.crawler.resolve(&html, &mut job.candidates[i]);
                    if job.candidates[i].specs_url.is_none() {
                        job.issue(&c.url, "探索", "未找到直接技術規格連結");
                    }
                }
                Err(e) => job.issue(&c.url, "探索", e),
            }
            self.store.save(job)?;
        }
        if job.options.discovery_only {
            return Ok(());
        }
        job.phase = "抽取產品規格".into();
        let mut counts: HashMap<String, usize> = HashMap::new();
        for c in job.candidates.clone() {
            if cancel.is_cancelled() {
                return Err("cancelled".into());
            }
            let count = counts.entry(c.category.clone()).or_default();
            if job.options.products_per_category > 0 && *count >= job.options.products_per_category
            {
                continue;
            }
            let Some(specs_url) = c.specs_url.as_ref() else {
                continue;
            };
            *count += 1;
            job.processed += 1;
            match self
                .extract(job, net.as_ref(), &c, specs_url, cancel, &mut cache)
                .await
            {
                Ok(product) => {
                    job.succeeded += 1;
                    job.products.push(product);
                }
                Err(e) => {
                    job.failed += 1;
                    job.issue(specs_url, "抽取", e);
                }
            }
            self.store.save(job)?;
        }
        Ok(())
    }
    async fn extract(
        &self,
        job: &mut Job,
        net: &dyn Fetcher,
        c: &Candidate,
        specs_url: &str,
        cancel: &CancellationToken,
        cache: &mut HashMap<String, String>,
    ) -> Result<Product, String> {
        let html = self
            .fetch(job, net, specs_url, "specs", &c.category, cache)
            .await?;
        let blocks = self.crawler.blocks(&html);
        if blocks.is_empty() {
            return Err("HTML 無可讀規格；可能需要瀏覽器或解析器更新".into());
        }
        let name = self.crawler.name(&html);
        let mut starting_price = cache
            .get(&c.url)
            .and_then(|h| self.crawler.price(h, &c.url));
        if starting_price.is_none()
            && let Some(buy) = &c.buy_url
        {
            match self.fetch(job, net, buy, "price", &c.category, cache).await {
                Ok(h) => starting_price = self.crawler.price(&h, buy),
                Err(e) => job.issue(buy, "售價", e),
            }
        }
        let mut product = Product {
            category: c.category.clone(),
            name,
            product_url: c.url.clone(),
            specs_url: specs_url.into(),
            fetched_at: now(),
            model: job.settings.model.clone(),
            variants: vec![],
            starting_price,
            specs: vec![],
            blocks: blocks.clone(),
            llm_calls: 0,
            llm_ms: 0,
            warnings: vec![],
        };
        if product.starting_price.is_none() {
            product
                .warnings
                .push("未取得可驗證的 TWD 起售價；不使用分期、折抵或模型猜測".into());
        }
        let folder = self
            .store
            .root
            .join(&c.category)
            .join(&job.id)
            .join(format!("product-{}", job.processed));
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let mut source_only = llm::fallback(
            &blocks
                .iter()
                .filter(|b| b.section == "註腳與限制")
                .cloned()
                .collect::<Vec<_>>(),
        );
        for spec in &mut source_only {
            spec.status = "source_only".into();
        }
        product.specs.extend(source_only);
        let batches = llm::batches(
            &blocks
                .iter()
                .filter(|b| b.section != "註腳與限制")
                .cloned()
                .collect::<Vec<_>>(),
        );
        for (i, batch) in batches.iter().enumerate() {
            if cancel.is_cancelled() {
                return Err("cancelled".into());
            }
            job.phase = format!("LLM 整理 {}（{}/{}）", product.name, i + 1, batches.len());
            job.current_url = specs_url.into();
            self.store.save(job)?;
            match self
                .annotator
                .annotate(
                    &job.settings.model,
                    &product.name,
                    batch,
                    cancel,
                    &folder,
                    i,
                )
                .await
            {
                Ok((specs, calls, ms)) => {
                    product.specs.extend(specs);
                    product.llm_calls += calls;
                    product.llm_ms += ms;
                }
                Err(e) => {
                    if cancel.is_cancelled() {
                        return Err(e);
                    }
                    job.issue(specs_url, "LLM", &e);
                    product.warnings.push(e);
                    product.specs.extend(llm::fallback(batch));
                }
            }
        }
        product.specs.sort_by_key(|s| s.block_id);
        product.variants = product
            .specs
            .iter()
            .filter_map(|s| s.model.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        if product.specs.iter().any(|s| s.model.is_none()) {
            product
                .warnings
                .push("部分區塊未能唯一對應型號，保留於未分配規格；未複製到各型號".into());
        }
        std::fs::write(
            folder.join("result.json"),
            serde_json::to_vec_pretty(&product).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(product)
    }
}
