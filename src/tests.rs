use super::*;
use actix_web::{http::StatusCode, test};

#[actix_web::test]
async fn browser_workflow() {
    let state = web::Data::new(Mutex::new(Vec::<Todo>::new()));
    let app = test::init_service(App::new().app_data(state).configure(routes)).await;
    let response =
        test::call_service(&app, test::TestRequest::get().uri("/todos").to_request()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = test::read_body(response).await;
    assert!(
        std::str::from_utf8(&body)
            .unwrap()
            .contains("Your list is empty")
    );

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/todos")
            .set_form([("title", "  Learn <Rust> & practice  ")])
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("Location").unwrap(), "/todos");

    for expected in ["class=\"done\"", "class=\"\""] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/todos/0/toggle")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let body =
            test::call_and_read_body(&app, test::TestRequest::get().uri("/todos").to_request())
                .await;
        let html = std::str::from_utf8(&body).unwrap();
        assert!(html.contains("Learn &lt;Rust&gt; &amp; practice"));
        assert!(html.contains(expected));
    }
}

#[actix_web::test]
async fn invalid_requests_do_not_change_the_list() {
    let state = web::Data::new(Mutex::new(Vec::<Todo>::new()));
    let app = test::init_service(App::new().app_data(state.clone()).configure(routes)).await;
    for title in ["   ".to_owned(), "x".repeat(121)] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/todos")
                .set_form([("title", title)])
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/todos/99/toggle")
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(state.lock().unwrap().is_empty());
}

#[actix_web::test]
async fn crawler_url_boundary_and_discovery() {
    use crate::apple::*;
    let base = "https://www.apple.com/tw/iphone-17/";
    assert_eq!(
        canonical(base, "specs/?x=1#size"),
        Some(format!("{base}specs/"))
    );
    for bad in [
        "http://www.apple.com/tw/",
        "https://www.apple.com.evil.test/tw/",
        "https://www.apple.com/us/",
        "https://127.0.0.1/tw/",
        "https://user@www.apple.com/tw/",
    ] {
        assert!(canonical(base, bad).is_none());
    }
    assert_eq!(
        category_for("https://www.apple.com/tw/macbook-future/"),
        Some("mac")
    );
    assert_eq!(
        category_for("https://www.apple.com/tw/iphone/compare/"),
        None
    );
    assert_eq!(
        links("<a href='specs/#x'>a</a><a href='specs/?x=1'>b</a>", base).len(),
        1
    );
}
#[actix_web::test]
async fn crawler_preserves_columns_and_qualifiers() {
    let blocks = crate::apple::blocks(include_str!("../tests/fixtures/specs.html"));
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0].model_hint.as_deref(), Some("型號 A"));
    assert!(blocks[0].text.contains("可選配：512GB"));
    assert_eq!(blocks[1].model_hint.as_deref(), Some("型號 B"));
    assert!(blocks[2].model_hint.is_none());
    assert!(blocks[2].text.contains("最長可達 18 小時"));
    assert!(!blocks[2].text.contains("ignore all"));
}
#[actix_web::test]
async fn crawler_price_ignores_promotions() {
    let price = crate::apple::price(
        include_str!("../tests/fixtures/price.html"),
        "https://www.apple.com/tw/example/",
    )
    .unwrap();
    assert_eq!(price.amount, 29900.0);
    assert_eq!(price.currency, "TWD");
    assert!(crate::apple::price("<p>每月 NT$999，折抵 NT$8000</p>", "url").is_none());
    assert!(
        crate::apple::price(
            &include_str!("../tests/fixtures/price.html").replace("TWD", "USD"),
            "url"
        )
        .is_none()
    );
}
#[actix_web::test]
async fn crawler_robots_specificity_and_wildcards() {
    use crate::network::Robots;
    let r = Robots::parse(
        "User-agent: *\nDisallow: /tw/shop/\nAllow: /tw/shop/buy-iphone/\nDisallow: /*overlay/*\n",
    );
    assert!(r.permits("/tw/iphone/specs/"));
    assert!(!r.permits("/tw/shop/bag/"));
    assert!(r.permits("/tw/shop/buy-iphone/"));
    assert!(!r.permits("/tw/overlay/test/"));
    let r = Robots::parse("User-agent: *\nDisallow: /\nUser-agent: RustCrawler\nAllow: /tw/\n");
    assert!(r.permits("/tw/iphone/"));
}
#[actix_web::test]
async fn crawler_llm_cannot_invent_values_or_assign_shared_models() {
    let blocks = crate::apple::blocks(include_str!("../tests/fixtures/specs.html"));
    let raw=serde_json::json!({"annotations":[{"block_id":0,"label":"容量","model":"假的型號","conditions":"可選配：512GB"},{"block_id":1,"label":"容量","model":null,"conditions":null},{"block_id":2,"label":"電池","model":"型號 A","conditions":null}]}).to_string();
    let result = crate::llm::validate(&raw, &blocks).unwrap();
    assert_eq!(result[0].value, blocks[0].text);
    assert_eq!(result[0].model.as_deref(), Some("型號 A"));
    assert!(result[2].model.is_none());
    assert_eq!(result[2].status, "needs_review");
    assert!(crate::llm::validate("not JSON", &blocks).is_err());
    assert!(crate::llm::validate("{\"annotations\":[]}", &blocks).is_err());
}
#[actix_web::test]
async fn crawler_batches_preserve_every_block() {
    let blocks = crate::apple::blocks(include_str!("../tests/fixtures/specs.html"));
    let batches = crate::llm::batches(&blocks);
    assert_eq!(batches.iter().map(Vec::len).sum::<usize>(), blocks.len());
    let fallback = crate::llm::fallback(&blocks);
    assert!(fallback.iter().all(|s| s.status == "llm_failed"));
    for (s, b) in fallback.iter().zip(&blocks) {
        assert_eq!(s.value, b.text);
    }
}
fn test_store() -> crate::storage::Store {
    crate::storage::Store::open(
        &std::env::temp_dir().join(format!("rust-crawler-test-{}", uuid::Uuid::new_v4())),
    )
    .unwrap()
}
#[actix_web::test]
async fn crawler_persistence_and_restart_recovery() {
    let store = test_store();
    let settings = crate::models::Settings {
        max_pages: 55,
        ..Default::default()
    };
    store.save_settings(&settings).unwrap();
    let mut job = crate::models::Job::new(settings, crate::models::RunOptions::default());
    job.succeeded = 1;
    store.save(&job).unwrap();
    let root = store.root.clone();
    drop(store);
    let store = crate::storage::Store::open(&root).unwrap();
    store.recover().unwrap();
    assert_eq!(store.settings().unwrap().max_pages, 55);
    let saved = store.job(&job.id).unwrap().unwrap();
    assert_eq!(saved.status, "interrupted");
    assert_eq!(saved.succeeded, 1);
    assert!(saved.finished_at.is_some());
}
#[actix_web::test]
async fn crawler_api_results_and_missing_job() {
    let store = test_store();
    let job = crate::models::Job::new(
        crate::models::Settings::default(),
        crate::models::RunOptions::default(),
    );
    store.save(&job).unwrap();
    let engine = web::Data::new(std::sync::Arc::new(crate::crawler::Engine::new(store)));
    let app = test::init_service(App::new().app_data(engine).configure(crate::api::routes)).await;
    for url in [
        format!("/api/jobs/{}", job.id),
        format!("/api/jobs/{}/download", job.id),
        "/api/settings".into(),
        "/api/jobs".into(),
    ] {
        let r = test::call_service(&app, test::TestRequest::get().uri(&url).to_request()).await;
        assert_eq!(r.status(), StatusCode::OK);
    }
    let r = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/jobs/missing")
            .to_request(),
    )
    .await;
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/jobs")
            .set_json(serde_json::json!({}))
            .to_request(),
    )
    .await;
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
}
#[actix_web::test]
async fn crawler_single_job_and_cancellation() {
    let engine = std::sync::Arc::new(crate::crawler::Engine::new(test_store()));
    let job = engine.start(crate::models::RunOptions::default()).unwrap();
    assert!(engine.start(crate::models::RunOptions::default()).is_err());
    engine.cancel(&job.id).unwrap();
    for _ in 0..50 {
        if engine.active_id().is_none() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        engine.store.job(&job.id).unwrap().unwrap().status,
        "cancelled"
    );
}

#[actix_web::test]
async fn crawler_buy_link_uses_this_products_navigation() {
    let html = "<a href='/tw/shop/goto/buy_ipad/ipad_air'>Other product</a><a class='ac-ln-button' href='/tw/shop/goto/buy_ipad/ipad'>購買</a>";
    assert_eq!(
        crate::apple::buy_link(html, "https://www.apple.com/tw/ipad-11/").as_deref(),
        Some("https://www.apple.com/tw/shop/goto/buy_ipad/ipad")
    );
    assert!(crate::apple::category_for("https://www.apple.com/tw/ipad-keyboards/").is_none());
    assert!(
        crate::apple::category_for("https://www.apple.com/tw/apple-watch-for-your-kids/").is_none()
    );
}

#[actix_web::test]
async fn crawler_source_footnotes_and_no_empty_sections() {
    let html = "<main><section><h2>價格</h2></section><section><h2>電池</h2><p>最長 18 小時</p></section></main><footer class='ac-gf-sosumi'><ol><li id='note-1'>測試結果依使用情況而異。</li></ol></footer>";
    let b = crate::apple::blocks(html);
    assert_eq!(b.len(), 2);
    assert_eq!(b[1].section, "註腳與限制");
    assert!(b[1].text.contains("依使用情況"));
}
#[actix_web::test]
async fn crawler_invalid_settings_and_invented_condition() {
    let mut settings = crate::models::Settings::default();
    settings.categories.clear();
    assert!(settings.validate().is_err());
    settings.categories = vec!["iphone".into()];
    settings.max_pages = 0;
    assert!(settings.validate().is_err());
    let blocks = crate::apple::blocks(include_str!("../tests/fixtures/specs.html"));
    let raw=serde_json::json!({"annotations":[{"block_id":0,"label":"容量","model":null,"conditions":"無限容量"}]}).to_string();
    let specs = crate::llm::validate(&raw, &blocks[..1]).unwrap();
    assert!(specs[0].conditions.is_none());
    assert_eq!(specs[0].status, "needs_review");
}

#[actix_web::test]
async fn crawler_mixed_layout_preserves_sections_without_duplicate_cells() {
    let blocks = crate::apple::blocks(include_str!("../tests/fixtures/mixed_specs.html"));
    let text = blocks
        .iter()
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(text.matches("256GB").count(), 1);
    assert_eq!(text.matches("512GB").count(), 1);
    assert!(text.contains("容量依設定而異"));
    assert!(text.contains("最長可達 18 小時"));
}

#[actix_web::test]
async fn crawler_native_colspan_is_never_assigned_to_one_model() {
    let html = "<table><tr><th role='columnheader'>A</th><th role='columnheader'>B</th></tr><tr><td colspan='2'>shared</td><td>other</td></tr></table>";
    let blocks = crate::apple::blocks(html);
    assert!(blocks[0].model_hint.is_none());
}

#[actix_web::test]
async fn crawler_nested_sections_preserve_parent_text_once() {
    let blocks = crate::apple::blocks(
        "<main><section><h2>外層</h2><p>parent qualifier</p><section><h3>內層</h3><p>child value</p></section></section></main>",
    );
    let text = blocks
        .iter()
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(text.matches("parent qualifier").count(), 1);
    assert_eq!(text.matches("child value").count(), 1);
}

#[actix_web::test]
async fn crawler_table_only_section_does_not_emit_header_only_fallback() {
    let fixture = include_str!("../tests/fixtures/specs.html")
        .replace("<h1>Sample 技術規格</h1>", "")
        .replace("<main>", "<main><section><h2>規格</h2>")
        .replace("</main>", "</section></main>");
    assert_eq!(crate::apple::blocks(&fixture).len(), 3);
}

#[actix_web::test]
async fn crawler_iphone_prices_preserve_each_model() {
    let url = "https://www.apple.com/tw/shop/buy-iphone/iphone-18-pro";
    let html = include_str!("../tests/fixtures/iphone-pro-prices.html");
    let prices = crate::apple::prices(html, url);
    assert_eq!(prices.len(), 2);
    assert_eq!(prices[0].source_name, "iPhone 18 Pro");
    assert_eq!(prices[0].amount, 44900.0);
    assert_eq!(prices[1].source_name, "iPhone 18 Pro Max");
    assert_eq!(prices[1].amount, 49900.0);
    for price in &prices {
        assert_eq!(price.source_url, url);
        assert_eq!(price.currency, "TWD");
        let evidence: serde_json::Value = serde_json::from_str(&price.evidence).unwrap();
        assert_eq!(evidence["lowPrice"].as_f64(), Some(price.amount));
    }
    assert_eq!(crate::apple::price(html, url).unwrap().amount, 44900.0);
    let duo = crate::apple::prices(
        include_str!("../tests/fixtures/iphone-duo-prices.html"),
        url,
    );
    assert_eq!(duo.len(), 1);
    assert_eq!(duo[0].source_name, "iPhone Duo");
    assert_eq!(duo[0].amount, 74900.0);
    assert!(crate::apple::prices(&html.replace("TWD", "USD"), url).is_empty());
    let values: Vec<serde_json::Value> = prices
        .iter()
        .map(|p| {
            serde_json::json!({
                "@type":"Product", "name":p.source_name,
                "offers":serde_json::from_str::<serde_json::Value>(&p.evidence).unwrap()
            })
        })
        .collect();
    let graph = format!(
        "<script type='application/ld+json'>{}</script>",
        serde_json::json!({"@graph":values})
    );
    assert_eq!(crate::apple::prices(&graph, url).len(), 2);
}

#[actix_web::test]
async fn crawler_modern_purchase_cta_ignores_other_products() {
    for slug in ["iphone-18-pro", "iphone-duo"] {
        let href = format!("/tw/shop/goto/buy_iphone/{}", slug.replace('-', "_"));
        let html = format!(
            "<a class='product-link' href='/tw/shop/goto/buy_iphone/iphone_16'>Other</a><a href='{href}' class='typography-caption cta buy'>查看價格</a>"
        );
        assert_eq!(
            crate::apple::buy_link(&html, &format!("https://www.apple.com/tw/{slug}/")),
            Some(format!("https://www.apple.com{href}"))
        );
    }
}
