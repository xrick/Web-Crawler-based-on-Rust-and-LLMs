# Apple 台灣產品規格爬蟲

以 Rust 2024、Actix Web、SQLite 與本機 Ollama 建立的產品規格爬蟲。支援 iPhone、Mac、iPad、Apple Watch、AirPods：先從 Apple 台灣官網探索產品，再擷取規格原文，最後由 LLM 分類。規格數值與證據由 Rust 複製原文，不讓模型重新編寫。

## 1. 啟動系統

### 環境需求

| 工具／服務 | 用途與需求 |
| --- | --- |
| Rust、Cargo | 使用支援 edition 2024 的工具鏈；開發檢查需要 rustfmt、Clippy |
| Ollama | 在 `http://127.0.0.1:11434` 提供模型服務；目前端點寫在 `llm::ENDPOINT` |
| `qwen2.5-coder:14b` | 預設模型；也可在設定頁選擇其他已安裝模型 |
| Python 3 | 執行驗證腳本與 Python 測試，僅使用標準函式庫 |
| 網路 | 實際爬取需能存取 Apple 官網；離線測試不需 Apple 或 Ollama |

以下命令都在儲存庫根目錄執行。

```sh
# 終端機 A：若 Ollama 尚未執行，啟動服務並保持開啟
ollama serve
```

```sh
# 終端機 B：首次使用時安裝模型
ollama pull qwen2.5-coder:14b

# 編譯並啟動爬蟲伺服器
cargo run --locked
```

開啟 http://127.0.0.1:8080。首次編譯或載入模型可能較久。`cargo run` 只啟動服務，不會自動建立爬取工作。

### 埠與資料根目錄

| 設定 | 預設值 | 說明 |
| --- | --- | --- |
| `CRAWLER_PORT` | `8080` | 有效範圍 1–65535；伺服器只綁定 `127.0.0.1` |
| `CRAWLER_DATA_DIR` | `crawler_data/apple` | 完整資料根目錄；相對路徑以啟動時的工作目錄為準 |

```sh
# 8080 已被使用時，以 8081 啟動
CRAWLER_PORT=8081 cargo run --locked

# 或同時指定資料根目錄
CRAWLER_PORT=8081 CRAWLER_DATA_DIR=crawler_data/apple cargo run --locked
```

使用 8081 時開啟 http://127.0.0.1:8081。避免兩個服務共用同一個資料根目錄：單工作互斥僅限同一個 Engine 程序。

## 2. 設定與抓取資料

### 透過網頁操作

1. 開啟 `/settings`，選擇類別、已安裝模型與頁數上限，按儲存。
2. 回到 `/` Dashboard，按「開始爬取」。按鈕建立一般工作，處理所有找到規格連結的候選產品，仍受頁數預算限制。
3. 頁面每兩秒查詢工作歷史與選定工作的進度，顯示已發現、已處理、已保存與失敗數。
4. 展開產品查看型號分組、規格原文、註腳、價格來源與待確認項目；可下載完整工作 JSON。
5. 需要中止時按「取消工作」。已下載頁面與已完成產品會保留，正在處理的產品不算完成。

| 設定／限制 | 行為 |
| --- | --- |
| 類別 | `iphone`、`mac`、`ipad`、`watch`、`airpods` |
| HTML 頁數 | 預設 100，允許 5–300；類別頁、產品頁、規格頁與購買頁共用預算 |
| 下載 | 首次嘗試前等待 1 秒，重試等待遞增；單次 HTTP 請求逾時 30 秒 |
| Ollama | 分批、依序呼叫；單次分類 HTTP 請求逾時 5 分鐘 |
| 並行工作 | 同一個 Engine 一次只執行一個工作 |
| 重啟 | 未完成工作標記為 `interrupted`，不自動續跑 |

### 透過腳本抓取並驗證

先啟動伺服器，再於另一個終端機執行。下列命令是不同模式的範例；每次執行都建立新工作，請等待前一個工作完成。

```sh
# 只探索產品與規格／購買連結，不呼叫 LLM 分類
python3 scripts/verify_live.py --mode discovery

# 每類最多嘗試一個有規格連結的產品，適合先做驗收
python3 scripts/verify_live.py --mode sample

# 嘗試全部候選產品；仍受設定頁的 max_pages 限制
python3 scripts/verify_live.py --mode full

# 若伺服器使用 8081
python3 scripts/verify_live.py --base-url http://127.0.0.1:8081 --mode sample
```

三種模式都使用伺服器已保存的類別設定。`sample` 的第一個產品若抽取失敗，不會自動改試同類下一個產品。`discovery` 的背景工作不需要 Ollama；但透過設定 API 儲存模型時仍需 Ollama 可用。

| 模式 | `RunOptions` | 驗證通過條件 |
| --- | --- | --- |
| `discovery` | `discovery_only=true`、`products_per_category=0` | 每個選定類別至少找到一個規格連結 |
| `sample` | `discovery_only=false`、`products_per_category=1` | 每個選定類別至少一個已抽取產品，且通過來源與分類檢查 |
| `full` | `discovery_only=false`、`products_per_category=0` | 每個候選產品都已抽取，且通過來源與分類檢查 |

```sh
# 觀察既有工作，不建立新工作；替換成實際 ID，mode 必須符合原工作
python3 scripts/verify_live.py --job <JOB_ID> --mode sample

# 自訂報告位置
python3 scripts/verify_live.py --mode sample --output-dir crawler_data/apple/verification
```

上例 `<JOB_ID>` 是佔位符，執行前必須替換。若伺服器設定了不同的 `CRAWLER_DATA_DIR`，驗證腳本也需設定相同環境變數或明確指定 `--output-dir`；腳本不會向伺服器推測磁碟位置。

驗證輸出包含 `<job_id>.json` 工作快照與 `<job_id>-quality.json` 品質報告。缺少類別、產品或來源區塊、重複 block ID、原文不一致、分類失敗時，腳本以非零狀態結束。`needs_review` 會列入統計但不直接判定失敗，仍需人工確認。

## 3. 開發與測試

HTML、JavaScript 以 `include_str!` 嵌入執行檔，沒有獨立前端建置流程。修改 `.rs`、`.html` 或 `.js` 後，以 Ctrl+C 停止開發服務，再執行 `cargo run --locked`。若工作正在執行，先在 Dashboard 取消並等候完成。

| 命令 | 用途 |
| --- | --- |
| `cargo build --locked` | 編譯，使用現有 `Cargo.lock` |
| `cargo run --locked` | 編譯並啟動開發服務 |
| `cargo fmt` | 套用 Rust 格式 |
| `cargo fmt --check` | 檢查格式而不修改檔案 |
| `cargo clippy --locked --all-targets -- -D warnings` | 檢查所有 Rust targets，警告視為失敗 |
| `cargo test --locked` | 執行 Rust 單元與流程測試 |
| `python3 -m unittest discover -s scripts -p 'test_*.py'` | 測試驗證腳本的品質判定 |

| 測試位置 | 測試方式與範圍 |
| --- | --- |
| `src/tests.rs`、`tests/fixtures/` | Actix 測試與固定 HTML；API、資料保存、來源保留、混合／巢狀區段、colspan、LLM 驗證 |
| `src/integration_tests.rs` | `Fixture` 注入 `FixtureDownloads`、`FixtureFetcher`、`FixtureModel`；測試頁數限制、推論中取消、模型與儲存失敗 |
| `src/network.rs` 的 `tests` | 隨機 loopback 埠的 HTTP fixture server；測試重試、逾時、重新導向與取消，執行環境須允許本機 socket |
| `scripts/test_verify_live.py` | Python unittest；驗證空結果、遺漏、重複、分類失敗及模式不符 |
| `.github/workflows/ci.yml` | Push／PR 時執行格式、Clippy、Rust 與 Python 測試 |

## 4. 系統架構

瀏覽器／驗證腳本 → HTTP API → `Engine` → 爬蟲能力與外部服務 → SQLite／分類資料目錄。API 與瀏覽器讀取持久化快照呈現進度。

### Module 與責任

| Module／檔案 | 核心 struct／trait／型別 | 責任與依賴 |
| --- | --- | --- |
| [main](src/main.rs) | `HttpServer`、`web::Data`、`Todo`、`NewTodo`、`Todos` | 設定埠、建立 Store／Engine、註冊路由、嵌入 HTML；保留獨立 to-do 範例 |
| [api](src/api.rs) | `State = web::Data<Arc<Engine>>` | HTTP 請求轉接至 Engine／Store；模型列表與儲存設定直接呼叫 `llm::models` |
| [crawler](src/crawler.rs) | `Engine` | 工作生命週期、快取、頁數預算、批次分類與保存 |
| [crawlers](src/crawlers.rs) | `Discovery`、`Extraction`、`Crawler`、`AppleTaiwanCrawler` | 網站能力介面；Apple 實作委派至 `apple` 函式 |
| [services](src/services.rs) | `DownloadFactory`、`Fetcher`、`Annotator`、`AppleDownloads`、`Ollama`、`IoFuture` | 可注入的 I/O 邊界；委派至 Downloader／llm |
| [network](src/network.rs) | `Downloader`、`Robots`、內部 `RobotGroup` 別名 | 限定 URL、robots 規則、HTTP、重試與取消 |
| [apple](src/apple.rs) | 使用 `scraper::Html`、`ElementRef`、`Selector`、`url::Url` | 純解析函式：探索連結、抽取區塊與 JSON-LD 價格 |
| [llm](src/llm.rs) | `Annotation`、內部 `Output` | Ollama JSON Schema、分批請求、結果驗證與 fallback |
| [models](src/models.rs) | `Settings`、`RunOptions`、`Job` 等 | API、Engine、SQLite 共用的可序列化資料 |
| [storage](src/storage.rs) | `Store`、`Mutex<rusqlite::Connection>` | SQLite 設定與工作 JSON 快照、重啟恢復 |
| [app.js](src/app.js)、`src/*.html` | DOM 節點、`Map`，無自訂 class | 設定表單、API 呼叫、輪詢、產品分組與安全文字顯示 |
| [verify_live.py](scripts/verify_live.py) | Python `dict`、`Counter`、`Path` | 建立／觀察工作、驗證品質、寫入報告 |

### 爬蟲能力階層與依賴注入

Rust 使用 trait 組合表達能力階層，並非 class 繼承。

| Trait／型別 | 方法 | 正式實作與用途 |
| --- | --- | --- |
| `Discovery` | `entry_url`、`candidates`、`resolve` | `AppleTaiwanCrawler`：類別入口、產品篩選、規格／購買連結 |
| `Extraction` | `blocks`、`name`、`price`、`plain` | `AppleTaiwanCrawler`：抽取來源內容 |
| `Crawler: Discovery + Extraction` | `robots_url`，並具備兩個父 trait 的方法 | `AppleTaiwanCrawler`：完整產品爬蟲 |
| `DownloadFactory` | `create` | `AppleDownloads` 建立 `Box<dyn Fetcher>`，具體為 Downloader |
| `Fetcher` | `robots`、`get` | Downloader 的 trait 實作在 `services.rs`；下載並載入 robots 或頁面 |
| `Annotator` | `models`、`annotate` | `Ollama`：取得模型列表及分類 |
| `IoFuture<'a, T>` | 非方法；boxed future 型別別名 | 讓 trait 方法回傳非同步 `Result<T, String>` |
| `Engine` | `new`、`with_components` | `new` 注入上述正式實作；`with_components` 接受替代策略或測試服務 |

`Arc<Engine>` 在 HTTP workers 間分享控制器；`active` 的 Mutex 保存目前工作 ID 與 `CancellationToken`。Store 用獨立 Mutex 保護短暫 SQLite 操作。鎖不跨網路 `.await`；下載與分類等待期間仍可處理 API 請求。

### Workflow 使用的資料 struct

| Struct／所在 module | 主要內容 | 產生與使用階段 |
| --- | --- | --- |
| `Settings`／models | `categories`、`model`、`max_pages` | `default` 提供預設值；`validate` 檢查；每個工作保存設定快照 |
| `RunOptions`／models | `discovery_only`、`products_per_category` | API 解析，Engine 決定探索或抽取範圍；不修改 Settings |
| `Job`／models | ID、狀態、時間、設定、計數、pages／candidates／products／issues | `Job::new` 建立；`Job::issue` 記錄問題；`models::now` 產生 UTC 時間 |
| `Candidate`／models | 類別、產品 URL、規格 URL、購買 URL | 類別探索建立，產品頁解析補齊 |
| `PageRecord`／models | URL、用途、類別、時間、HTML／文字相對路徑、SHA-256 | `Engine::fetch` 成功保存下載內容後建立 |
| `Block`／models | `id`、section、model_hint、context、text | `apple::blocks` 產生；作為分類及來源驗證依據 |
| `Price`／models | amount、currency、source_url、source_name、evidence、scope | 官方 Product JSON-LD 提供可驗證 TWD 價格時建立 |
| `Annotation`、`Output`／llm | 單一標註；標註陣列容器 | 反序列化 LLM JSON；由 `llm::validate` 檢查 |
| `Specification`／models | block_id、分類、原文 value／evidence、model、conditions、status | 正常分類、來源註腳或 fallback；對應一個 Block |
| `Product`／models | 名稱、來源、模型、variants、price、specs、blocks、LLM 統計與 warnings | `Engine::extract` 組裝，保存 `result.json` 並加入 Job |
| `Issue`／models | URL、stage、message | 探索、下載、分類或工作失敗時加入 Job |
| `Store`／storage | SQLite 連線 Mutex、資料 root | 啟動時建立；設定與工作狀態的持久化入口 |
| `Downloader`、`Robots`／network | HTTP client、取消 token、規則及重試等待 | 每個工作建立 downloader，再載入 robots 規則 |

## 5. Workflow 與 function 對照

以下以正式執行路徑為準，列出專案自訂 function／method；標準庫及第三方 API 不逐一展開。`Type::method` 表示方法，`module::function` 表示模組函式。

### 啟動、設定與建立工作

| 步驟 | Module | Struct／trait | Function／method 呼叫 | 輸入 → 結果 |
| --- | --- | --- | --- | --- |
| 1. 啟動 | main、storage | Store、Engine、HttpServer | `main` → `server_port`、`Store::open`、`Store::recover`、`Engine::new` → `Engine::with_components` | 環境變數 → DB、共用引擎與 loopback 服務 |
| 2. 恢復舊工作 | storage、models | Store、Job | `Store::recover` → `Store::jobs`、`models::now`、`Store::save` | running／cancelling 快照 → interrupted |
| 3. 註冊／顯示介面 | main、api | web::Data、State | `main::routes`、`api::routes`；`index`／`settings` → `crawler_page`；`api::script` | `/`、`/settings`、`/app.js` → 嵌入資產 |
| 4. 載入設定與模型 | app.js、api、storage、llm | Settings、Store、Ollama JSON | JS `api` → `api::get_settings` → `Store::settings`／`Settings::default`；`api::models` → `llm::models` | 已保存設定與模型列表 → 表單 |
| 5. 儲存設定 | app.js、api、models、storage | Settings、Store | 表單 submit → JS `api` → `api::save_settings` → `mutation_allowed`、`Settings::validate`、`llm::models`、`Store::save_settings` | JSON → 驗證模型已安裝後寫入 SQLite |
| 6. 建立工作 | app.js、api、crawler、models | RunOptions、Engine、Job | start onclick → `api::start` → `mutation_allowed` → `Engine::start` → `Store::settings`、`Settings::validate`、`Job::new`、`Store::save` | `{}` 或 RunOptions → HTTP 202、背景 task |
| 7. 啟動背景流程 | crawler | Engine、CancellationToken | `Engine::run` → `Engine::work` | Job 快照 → 探索／抽取；結束時統一處理狀態 |

### 下載、探索、抽取與分類

| 步驟 | Module | Struct／trait | Function／method 呼叫 | 輸入 → 結果 |
| --- | --- | --- | --- | --- |
| 8. 建立下載器 | services、network | DownloadFactory、AppleDownloads、Downloader | `DownloadFactory::create` → `Downloader::new` | 取消 token → HTTP downloader |
| 9. robots 與模型檢查 | crawlers、services、network、llm | Crawler、Fetcher、Robots、Annotator | `Crawler::robots_url` → `Fetcher::robots` → `Downloader::get`、`Robots::parse`；非 discovery 時 `Annotator::models` → `llm::models` | 規則檔與模型列表 → 允許的下載政策與模型可用性 |
| 10. 類別入口 | crawler、crawlers、apple | Engine、Discovery、Candidate | `Discovery::entry_url` → `Engine::fetch`；`Discovery::candidates` → `apple::links`、`apple::category_for` | 類別 HTML → 去重候選 URL |
| 11. 通用下載 | crawler、services、network、apple | Engine、Fetcher、Downloader、Robots、PageRecord | `Engine::fetch` → `Fetcher::get` → `Downloader::get` → `apple::allowed`、`Robots::permits`；`Extraction::plain` → `apple::plain`；`Store::save` | 快取／頁數檢查 → 下載、SHA-256、HTML／文字檔與 PageRecord |
| 12. 補齊產品連結 | crawler、crawlers、apple | Candidate、Discovery | `Engine::work` 依類別輪流排列候選；`Engine::fetch` → `Discovery::resolve` → `apple::links`、`apple::buy_link` | 產品頁 → specs_url／buy_url；缺少規格連結則 `Job::issue` |
| 13. 選擇抽取產品 | crawler | RunOptions、Candidate、Job | `Engine::work` → `Engine::extract` | discovery 在此之前結束；其餘按每類上限及規格連結進行抽取 |
| 14. 規格與名稱 | crawler、crawlers、apple | Extraction、Block、Product | `Engine::fetch` → `Extraction::blocks`／`name` → `apple::blocks`／`product_name` | 規格 HTML → 表格、區段、註腳與名稱；空區塊視為抽取失敗 |
| 15. 價格來源 | crawler、crawlers、apple | Extraction、Price | `Extraction::price` → `apple::price` → `product_price`；必要時 `Engine::fetch` 購買頁 | 產品／購買頁 JSON-LD → TWD 起售價或 None |
| 16. 分批 | crawler、llm | Block、Specification | 註腳 `llm::fallback` 後標為 source_only；其他區塊 `llm::batches` | 每批最多 16 區塊或累計約 6500 字元；單一長區塊已由解析器分割 |
| 17. 呼叫模型 | crawler、services、llm | Annotator、Ollama、Annotation、Output | `Annotator::annotate` → `llm::annotate` → `schema`；使用 `SYSTEM` 提示詞 | 保存 request → Ollama chat → 保存 response；含傳輸重試與驗證修正輪次 |
| 18. 驗證／保留原文 | llm、apple | Output、Block、Specification | `llm::validate` → `apple::clean`；失敗時 Engine 呼叫 `Job::issue`、`llm::fallback` | 檢查 ID、數量、標籤與條件；value／evidence 複製 Block 原文 |
| 19. 保存產品 | crawler、storage | Product、Job、Store | `Engine::extract` 寫入 `result.json`；`Engine::work` 更新計數並 `Store::save` | 完成產品 → 類別資料夾與 Job 快照 |
| 20. 結束工作 | crawler、models、storage | Job、Engine、Store | `Engine::run` → `Job::issue`（必要時）、`models::now`、`Store::save` | completed／completed_with_errors／failed／cancelled；寫入結束時間並釋放 active |

### 解析輔助函式

| Module | Function | 在上述流程的作用 |
| --- | --- | --- |
| apple | `sel` | 建立靜態 CSS Selector，供連結、區塊、名稱與價格解析使用 |
| apple | `clean`、`text` | 正規化空白、保留段落界線，排除 script／style／noscript／sup 內容 |
| apple | `allowed`、`canonical` | 限定 HTTPS、Apple host／台灣路徑；解析相對 URL，移除 query／fragment |
| apple | `links`、`category_for`、`buy_link` | 去重頁面連結、判斷產品類別、從產品導覽 CTA 找購買網址 |
| apple | `product_name`、`plain` | 從 h1／title 取得名稱，從 main／body 產生純文字 |
| apple | `blocks` | 抽取表格與各 section 自有內容，避免巢狀重複；處理 colspan、註腳與 3500 字元分割 |
| apple | `price`、`product_price` | 解析 JSON-LD 陣列／graph／Product offers，取得有效 TWD 起售價 |
| network | `Robots::parse`、`Robots::permits` | 選擇 RustCrawler 或萬用規則群組，以路徑、萬用字元與規則優先序判斷 |
| api | `mutation_allowed`、`failure`、`internal` | 自訂 header／Origin 檢查；產生 HTTP 錯誤 JSON |

### 查詢、取消、匯出與品質驗證

| 操作 | Module | Struct／型別 | Function／method 呼叫與結果 |
| --- | --- | --- | --- |
| 輪詢歷史 | app.js、api、storage、crawler | Job、Store、Engine | JS `refresh` → `api::jobs` → `Store::jobs`、`Engine::active_id`；回傳摘要及執行中 ID |
| 查看單一工作 | app.js、api、storage | Job、Product、Specification | `refresh` → `api::job` → `Store::job`；JS `products` 按型號分組並顯示結果 |
| 畫面輔助 | app.js | DOM、Map | `$` 取元素；`api` 呼叫 JSON；`message` 顯示訊息；`node` 以 textContent 建立節點；事件處理器呼叫上述函式 |
| 取消 | app.js、api、crawler | Engine、CancellationToken | cancel onclick → `api::cancel` → `mutation_allowed`、`Engine::cancel`；token 中斷等待，`Engine::run` 最終標記 cancelled |
| 下載 JSON | api、storage | Job | `api::download` → `Store::job`；以 attachment 回傳工作 JSON |
| 腳本啟動／觀察 | scripts/verify_live.py | argparse Namespace、dict | `main` → `request`：POST 建立或 GET 觀察；輪詢直到非 running／cancelling |
| 品質報告 | scripts/verify_live.py | dict、Counter、Path | `main` → `validate_job`；寫工作快照、分類狀態統計與 errors；未通過則 exit 1 |

取消 API 回覆的 `cancelling` 是請求接受訊息；目前不另外保存這個中間狀態，工作停止後才寫入 `cancelled`。最終 SQLite 保存失敗時會記錄 stderr，資料庫可能仍停留在較早快照；品質報告不能取代儲存錯誤檢查。

### 獨立的 to-do 教學流程

| Module | Struct／型別 | Function／method | 行為 |
| --- | --- | --- | --- |
| main | `Todo`、`NewTodo`、`Todos` | `todo_index` → `escape_html` | `/todos` 顯示記憶體清單並跳脫使用者文字 |
| main | NewTodo、Todos | `add_todo` → `back_to_list` | 表單新增後重新導向 |
| main | Todos | `toggle_todo` → `back_to_list` | 切換完成狀態後重新導向 |

to-do 清單不使用爬蟲 Engine 或 SQLite，重啟即清空。

## 6. HTTP API 對照

寫入請求需帶 `X-Crawler-Request: 1`；JSON body 端點需帶 `Content-Type: application/json`。瀏覽器 Origin 若存在，必須是使用設定埠的 localhost／127.0.0.1。

| 方法與路徑 | Handler（api module） | 用途 |
| --- | --- | --- |
| `GET /api/models` | `models` | 已安裝 Ollama 模型 |
| `GET /api/settings` | `get_settings` | 目前設定 |
| `POST /api/settings` | `save_settings` | 儲存 `{categories, model, max_pages}` |
| `POST /api/jobs` | `start` | `{}` 為一般工作；也接受 RunOptions；成功 202，Engine 啟動錯誤目前統一 409 |
| `GET /api/jobs` | `jobs` | 歷史摘要及 active_id |
| `GET /api/jobs/{id}` | `job` | 完整工作，找不到時 404 |
| `POST /api/jobs/{id}/cancel` | `cancel` | 接受取消回覆 202；非執行中工作回覆 409 |
| `GET /api/jobs/{id}/download` | `download` | 工作 JSON 附件 |

## 7. 資料目錄與品質語意

```text
crawler_data/apple/
├── crawler.sqlite3
├── runs/{job_id}/robots.txt
├── iphone/{job_id}/
│   ├── page-0.html
│   ├── page-0.txt
│   └── product-1/
│       ├── result.json
│       ├── llm-0-0-request.json
│       └── llm-0-0-response.json
├── mac/{job_id}/...
├── ipad/{job_id}/...
├── watch/{job_id}/...
├── airpods/{job_id}/...
└── verification/
    ├── {job_id}.json
    └── {job_id}-quality.json
```

類別子目錄依實際下載建立。`page-N` 是工作內下載頁序號，`product-N` 是工作內抽取嘗試序號，不是每個類別從 1 重編；上圖僅示意。PageRecord 的檔案路徑相對於資料 root。不同 job ID 避免覆蓋先前工作；robots 為工作共用。

| Specification status | 意義 |
| --- | --- |
| `source_matched` | 已保留來源原文；不代表分類或事實已經人工驗證 |
| `source_only` | 註腳與限制直接保留來源，不經模型分類 |
| `needs_review` | 模型提出無法確認的型號，或條件片段驗證不通過，需要人工確認 |
| `llm_failed` | 分類失敗，仍以 fallback 保留來源區塊 |

表格明確型號優先於模型建議，共用欄位不任意分配。價格只接受官方 Product JSON-LD 的 TWD `lowPrice`／`price`，找不到時為 null，不使用分期、折抵或模型推測。起售價屬於來源商品頁，不自動套用每個子型號。`model_prices` 保存頁面所有 Product JSON-LD 的具名起售價（例如 Pro 與 Pro Max），Dashboard 分別顯示；`starting_price` 保留最低起售價供摘要使用。購買連結支援舊版 `ac-ln-button` 與新版 `cta buy`。舊工作不會自動補抓價格，需重新建立工作。

`target/`、`data/`、`crawler/`、`crawler_data/` 已由 `.gitignore` 排除；固定 fixtures 與 `output/verification/initial_run.json` 歷史範例保留。舊 `data/` 不自動搬移，可用 `CRAWLER_DATA_DIR=data cargo run --locked` 查看舊工作。既有報告為 `output/pdf/apple_tw_crawler_report.pdf`；原報告產生器 `scripts/build_report.py` 未包含於目前儲存庫。

## 8. 擴充方式與目前限制

| 擴充目標 | 修改位置與方式 |
| --- | --- |
| 不同解析策略 | 實作 Discovery、Extraction、Crawler，透過 `Engine::with_components` 注入 |
| 其他網站 | 除 crawler 外，提供對應 DownloadFactory／Fetcher 的 URL 與 robots 政策，調整 Settings 類別及 UI；目前正式實作只有 AppleTaiwanCrawler |
| 其他分類服務 | 實作 Annotator 並注入 Engine；設定／模型列表 API 目前直接呼叫 `llm::models`，也需同步調整 |
| 新增回歸案例 | 解析測試放 `src/tests.rs` 與 fixtures；流程測試使用可控下載器與分類器；品質判定測試放 Python unittest |

目前是本機單人工具，沒有帳號、排程、多工作並行、自動續跑或雲端部署。僅抓 Apple 台灣範圍；若網站改版、拒絕存取或必須執行 JavaScript，需檢查缺漏。快取只在同一工作內重用，每次新工作重新下載。SQLite 操作同步執行，歷史查詢會讀取完整工作 JSON，大量資料時仍有優化空間。
