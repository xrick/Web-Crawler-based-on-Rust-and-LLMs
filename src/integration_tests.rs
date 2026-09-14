//! In-process pipeline tests: no Apple or Ollama connections.
use crate::{
    crawler::Engine, crawlers::AppleTaiwanCrawler, models::*, services::*, storage::Store,
};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio_util::sync::CancellationToken;

struct FixtureDownloads {
    prices: bool,
    calls: Arc<AtomicUsize>,
}
struct FixtureFetcher {
    prices: bool,
    calls: Arc<AtomicUsize>,
}
impl DownloadFactory for FixtureDownloads {
    fn create(&self, _: CancellationToken) -> Result<Box<dyn Fetcher>, String> {
        Ok(Box::new(FixtureFetcher {
            prices: self.prices,
            calls: self.calls.clone(),
        }))
    }
}
impl Fetcher for FixtureFetcher {
    fn robots<'a>(&'a mut self, _: &'a str) -> IoFuture<'a, String> {
        Box::pin(async { Ok("User-agent: *\nAllow: /".into()) })
    }
    fn get<'a>(&'a self, url: &'a str) -> IoFuture<'a, (String, String)> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let html = if self.prices && url.contains("/shop/") {
                include_str!("../tests/fixtures/iphone-pro-prices.html").into()
            } else if url.ends_with("/iphone/") {
                "<a href='/tw/iphone-a/'>A</a><a href='/tw/iphone-b/'>B</a><a href='/tw/iphone-c/'>C</a>".into()
            } else if url.ends_with("/specs/") {
                include_str!("../tests/fixtures/specs.html").into()
            } else {
                format!(
                    "<a href='{url}specs/'>Specs</a>{}",
                    if self.prices {
                        "<a class='cta buy' href='/tw/shop/buy-iphone/iphone-18-pro'>Buy</a>"
                    } else {
                        ""
                    }
                )
            };
            Ok((url.into(), html))
        })
    }
}
struct FixtureModel {
    entered: Arc<AtomicUsize>,
    wait: bool,
    fail: bool,
}
impl Annotator for FixtureModel {
    fn models(&self) -> IoFuture<'_, Vec<String>> {
        Box::pin(async { Ok(vec![Settings::default().model]) })
    }
    fn annotate<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        blocks: &'a [Block],
        cancel: &'a CancellationToken,
        _: &'a Path,
        _: usize,
    ) -> IoFuture<'a, (Vec<Specification>, usize, u128)> {
        Box::pin(async move {
            self.entered.fetch_add(1, Ordering::SeqCst);
            if self.wait {
                cancel.cancelled().await;
                return Err("cancelled".into());
            }
            if self.fail {
                return Err("model timeout".into());
            }
            let raw = serde_json::json!({"annotations": blocks.iter().map(|b| serde_json::json!({"block_id":b.id,"label":"規格","model":null,"conditions":null})).collect::<Vec<_>>()});
            Ok((crate::llm::validate(&raw.to_string(), blocks)?, 1, 1))
        })
    }
}
struct Fixture {
    engine: Arc<Engine>,
    calls: Arc<AtomicUsize>,
    entered: Arc<AtomicUsize>,
}
impl Fixture {
    fn new(max_pages: usize, wait: bool, fail: bool) -> Self {
        Self::with_prices(max_pages, wait, fail, false)
    }
    fn with_prices(max_pages: usize, wait: bool, fail: bool, prices: bool) -> Self {
        let root =
            std::env::temp_dir().join(format!("crawler-integration-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&root).unwrap();
        store
            .save_settings(&Settings {
                categories: vec!["iphone".into()],
                max_pages,
                ..Default::default()
            })
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(AtomicUsize::new(0));
        let engine = Arc::new(Engine::with_components(
            store,
            Arc::new(AppleTaiwanCrawler),
            Arc::new(FixtureDownloads {
                prices,
                calls: calls.clone(),
            }),
            Arc::new(FixtureModel {
                entered: entered.clone(),
                wait,
                fail,
            }),
        ));
        Self {
            engine,
            calls,
            entered,
        }
    }
    async fn finished(&self, id: &str) -> Job {
        tokio::time::timeout(Duration::from_secs(2), async {
            while self.engine.active_id().is_some() {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("worker did not terminate");
        self.engine.store.job(id).unwrap().unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.engine.store.root);
    }
}
#[actix_web::test]
async fn pipeline_preserves_sources_and_enforces_page_budget() {
    let f = Fixture::new(5, false, false);
    let job = f.engine.start(RunOptions::default()).unwrap();
    let job = f.finished(&job.id).await;
    assert_eq!(f.calls.load(Ordering::SeqCst), 5);
    assert_eq!(job.pages.len(), 5);
    for page in &job.pages {
        assert!(page.html_file.starts_with(&format!("iphone/{}/", job.id)));
        assert!(f.engine.store.root.join(&page.html_file).is_file());
        assert!(f.engine.store.root.join(&page.text_file).is_file());
    }
    assert!(
        f.engine
            .store
            .root
            .join("iphone")
            .join(&job.id)
            .join("product-1/result.json")
            .is_file()
    );
    assert!(
        f.engine
            .store
            .root
            .join("runs")
            .join(&job.id)
            .join("robots.txt")
            .is_file()
    );
    assert_eq!(job.succeeded, 1);
    assert_eq!(job.failed, 2);
    assert_eq!(job.status, "completed_with_errors");
    for p in job.products {
        for s in p.specs {
            assert_eq!(s.value, p.blocks[s.block_id].text);
        }
    }
    assert!(job.issues.iter().any(|i| i.message.contains("頁數上限")));
}
#[actix_web::test]
async fn pipeline_cancels_during_model_inference_and_releases_slot() {
    let f = Fixture::new(100, true, false);
    let job = f.engine.start(RunOptions::default()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while f.entered.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    f.engine.cancel(&job.id).unwrap();
    let job = f.finished(&job.id).await;
    assert_eq!(job.status, "cancelled");
    assert!(job.products.is_empty());
    assert!(!job.pages.is_empty());
    let next = f.engine.start(RunOptions::default()).unwrap();
    f.engine.cancel(&next.id).unwrap();
    f.finished(&next.id).await;
}
#[actix_web::test]
async fn pipeline_model_failure_preserves_every_source_block() {
    let f = Fixture::new(100, false, true);
    let job = f
        .engine
        .start(RunOptions {
            products_per_category: 1,
            ..Default::default()
        })
        .unwrap();
    let job = f.finished(&job.id).await;
    assert_eq!(job.status, "completed_with_errors");
    assert_eq!(job.products.len(), 1);
    let p = &job.products[0];
    assert_eq!(p.specs.len(), p.blocks.len());
    assert!(p.specs.iter().all(|s| s.status == "llm_failed"));
}
#[actix_web::test]
async fn persistence_failure_does_not_reserve_worker_slot() {
    let f = Fixture::new(100, false, false);
    let connection =
        rusqlite::Connection::open(f.engine.store.root.join("crawler.sqlite3")).unwrap();
    connection.execute_batch("CREATE TRIGGER reject_job BEFORE INSERT ON jobs BEGIN SELECT RAISE(FAIL, 'simulated disk failure'); END;").unwrap();
    assert!(
        f.engine
            .start(RunOptions::default())
            .unwrap_err()
            .contains("simulated disk failure")
    );
    assert!(f.engine.active_id().is_none());
    assert!(f.engine.store.jobs().unwrap().is_empty());
}
#[actix_web::test]
async fn artifact_write_failure_is_reported_and_releases_worker_slot() {
    let f = Fixture::new(100, false, false);
    std::fs::write(
        f.engine.store.root.join("runs"),
        "blocks directory creation",
    )
    .unwrap();
    let job = f.engine.start(RunOptions::default()).unwrap();
    let job = f.finished(&job.id).await;
    assert_eq!(job.status, "failed");
    assert!(!job.issues.is_empty());
}

#[actix_web::test]
async fn pipeline_persists_all_purchase_page_prices() {
    let f = Fixture::with_prices(20, false, false, true);
    let job = f
        .engine
        .start(RunOptions {
            products_per_category: 1,
            discovery_only: false,
        })
        .unwrap();
    let job = f.finished(&job.id).await;
    assert_eq!(job.status, "completed");
    let product = &job.products[0];
    assert_eq!(product.model_prices.len(), 2);
    assert_eq!(product.model_prices[1].source_name, "iPhone 18 Pro Max");
    assert_eq!(product.starting_price.as_ref().unwrap().amount, 44900.0);
    assert!(job.pages.iter().any(|p| p.kind == "price"));
    let path = f
        .engine
        .store
        .root
        .join(format!("iphone/{}/product-1/result.json", job.id));
    let saved: Product = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(saved.model_prices[1].amount, 49900.0);
    let mut legacy = serde_json::to_value(product).unwrap();
    legacy.as_object_mut().unwrap().remove("model_prices");
    assert!(
        serde_json::from_value::<Product>(legacy)
            .unwrap()
            .model_prices
            .is_empty()
    );
}
