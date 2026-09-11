# Apple 台灣產品規格爬蟲

Rust + Actix Web + 本機 Ollama。涵蓋 iPhone、Mac、iPad、Apple Watch、AirPods，從目前官網入口探索產品，保留技術規格原文，再讓 LLM 整理分類。SQLite 保存設定與每次工作；原始 HTML 與模型輸入／輸出保存於本機。

## 啟動

需求：Rust、Cargo、已啟動的 Ollama，以及 `qwen2.5-coder:14b`（也可在設定頁選擇其他已安裝模型）。

```sh
# 若 Ollama 尚未啟動，在另一個終端執行
ollama serve

# 若尚未安裝預設模型
ollama pull qwen2.5-coder:14b

cargo run
```

開啟 http://127.0.0.1:8080。第一次編譯或第一次載入模型可能較久。修改 `.rs`、HTML 或 JS 後，以 Ctrl+C 停止，再執行 `cargo run`。HTML/JS 使用 `include_str!` 嵌入可執行檔。

1. 在「爬蟲設定」選擇類別、模型及頁數上限並儲存。
2. 回到 Dashboard，按「開始爬取」。一次只執行一個工作。
3. 頁面每兩秒更新；探索完成前沒有假定的百分比進度。
4. 展開產品查看分類、型號分組、原文與價格來源，或下載完整工作 JSON。
5. 按「取消工作」可停止未完成操作，已完成產品仍保留。重啟時原先未完成工作標記為中斷，不會自動重跑。

預設 100 個 HTML 頁面，包含分類、產品、規格及購買頁；最多可設為 300。抓取間隔至少一秒，HTTP 逾時 30 秒，Ollama 每次請求逾時五分鐘。模型請求逐一執行，避免多個大型推論競爭記憶體。

## 程式導讀

| 模組 | 責任 |
| --- | --- |
| `src/main.rs` | 啟動伺服器、共用版面與原本的 to-do 範例 |
| `src/api.rs` | 設定、啟動、取消、查詢、下載的 HTTP 介面 |
| `src/crawler.rs` | 背景工作生命週期、探索、逐頁處理、持久化進度 |
| `src/network.rs` | 固定 Apple 台灣範圍、robots.txt、重新導向、逾時／重試／取消 |
| `src/apple.rs` | URL 去重、Apple 表格與欄位擷取、JSON-LD 起售價 |
| `src/llm.rs` | Ollama JSON Schema、提示詞、分類驗證及原文保留 |
| `src/models.rs` | `Settings`、`Job`、`Product`、`Block` 等可序列化資料型別 |
| `src/storage.rs` | SQLite 的設定與工作快照，重啟恢復 |
| `src/app.js` | 呼叫 JSON API、輪詢進度、以 `textContent` 安全顯示原文 |

### 一個請求怎麼走

瀏覽器 `POST /api/jobs` → API 建立工作 → SQLite 保存 `running` → 背景 task 下載 Apple → 解析規格區塊 → Ollama 分類 → Rust 驗證並從原文複製值 → 保存產品 → 瀏覽器 `GET /api/jobs/{id}` 顯示結果。

`async` 讓下載／推論等待期間伺服器仍可處理其他請求。`Arc<Engine>` 分享工作控制器；`Mutex` 只包住短暫狀態或 SQLite 操作，不跨網路 `.await`。`CancellationToken` 讓等待中的網路請求也能取消。

## LLM 的角色與資料品質

每個來源區塊有 `id`、原始段落、表格欄位的 `model_hint`、上下文與文字。LLM 回傳每區塊的分類標籤、型號建議與條件片段。Rust 最終的 `value` 和 `evidence` 都直接複製來源文字，因此模型不會重寫容量、尺寸或價格。

- `source_matched`：已保留來源原文；**不是事實正確率或人工驗證標章**。分類標籤仍可能錯誤。
- `needs_review`：模型提出了無法由獨立欄位確認的型號對應。保留於未分配組，不複製到所有型號。
- `llm_failed`：模型失敗時仍保留原始段落，避免丟失資料；工作會記錄錯誤。

表格已明確標示的型號優先於 LLM。共用 colspan 或含多型號的混合區塊保持未分配。Mac 官網的「機型 1／2／3」保留搭配表格上下文，並不虛構商品 SKU。長區塊分批處理，所有區塊都必須有結果；不把每段規格強制拆成單一數值欄位。

起售價只接受官方 Product JSON-LD 的 TWD `lowPrice`／`price`，保留商品名稱、offers 片段與 URL。頁面沒有這種可驗證資料時價格為 `null`，不採用分期、折抵價或 LLM 推測。起售價適用於來源商品頁，不自動套用成每個子型號價格。

## HTTP API

寫入操作須帶 `Content-Type: application/json` 與 `X-Crawler-Request: 1`。本機瀏覽器自動附加；拒絕非本機來源。資料錯誤請查看 JSON `error`。

| 方法與路徑 | 用途 |
| --- | --- |
| `GET /api/models` | 取得本機 Ollama 已安裝模型 |
| `GET /api/settings` | 讀取設定 |
| `POST /api/settings` | 儲存 `{categories, model, max_pages}` |
| `POST /api/jobs` | 以 `{}` 啟動，回覆 202 與工作 ID；已有工作回覆 409 |
| `GET /api/jobs` | 歷史摘要及 `active_id` |
| `GET /api/jobs/{id}` | 完整工作進度、來源、產品、錯誤 |
| `POST /api/jobs/{id}/cancel` | 取消目前工作 |
| `GET /api/jobs/{id}/download` | JSON 附件下載 |

啟動選項 `discovery_only: true` 只探索所有選定類別與規格／售價連結；`products_per_category: 1` 為每類一個產品的快速驗證，預設 `0` 代表全部。這兩個選項供腳本驗證，不會改掉已儲存設定。

## 資料位置與限制

- `crawler_data/apple/crawler.sqlite3`：SQLite，設定與工作 JSON 快照。
- `crawler_data/apple/{category}/{job_id}/`：原始 HTML、純文字、LLM 請求／回應與產品結果；共用 robots.txt 位於 `crawler_data/apple/runs/{job_id}/`。
- 可用 `CRAWLER_DATA_DIR` 指定資料根目錄。資料與測試快取不納入 Git。
- 僅本機單人用途；無帳號、排程、雲端部署或自動恢復。每次執行重新抓取；工作內相同 URL 才重用快取。
- 限 `https://www.apple.com/tw/`，不抓歷史支援頁、教育商店、一般配件或海外頁面。網站若改版、拒絕存取或必須執行 JS，記錄缺漏。
- SQLite 儲存短操作目前同步執行；適合此單工作教學專案，大量歷史資料時可再改為獨立儲存執行緒及分表查詢。
- 取消時保留已保存頁面及已完成產品；正在處理的產品不會當作完成結果。

原始 Rust 表單教學保留於 http://127.0.0.1:8080/todos，資料仍只在記憶體。可從這個小範例理解 route、handler、form、redirect，再讀爬蟲模組。

## 驗證與報告

```sh
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings

# 需要本機伺服器、Apple 網路與 Ollama
python3 scripts/verify_live.py --mode discovery
python3 scripts/verify_live.py --mode sample
# 全部產品，執行時間視模型與產品數而定
python3 scripts/verify_live.py --mode full
```

測試涵蓋來源保留、多欄位、价格排除、robots 規則、URL 邊界、LLM 格式與型號驗證、持久化、中斷恢復、單工作互斥及取消。新連網驗證證據放在 `crawler_data/apple/verification/`（歷史範例仍在 `output/verification/`），PDF 位於 `output/pdf/apple_tw_crawler_report.pdf`。

舊報告產生器 `scripts/build_report.py` 目前未包含於儲存庫；上述 PDF 為既有報告。

參考：[Actix state](https://actix.rs/docs/application/)、[Ollama structured outputs](https://docs.ollama.com/capabilities/structured-outputs)、[Apple robots.txt](https://www.apple.com/robots.txt)。原始 Rust 教學基於所附書籍第 5–7 章，爬蟲為本專案新增實作。

## 開發與擴充爬蟲

Rust 以 trait 組合表達爬蟲能力階層，避免將網站規則放入工作控制器：

- `src/crawlers.rs`：`Discovery` 定義入口、候選產品及規格／購買連結探索；`Extraction` 定義規格、名稱、價格與純文字擷取。`Crawler: Discovery + Extraction` 結合兩者並指定 robots URL，`AppleTaiwanCrawler` 是目前的實作。
- `src/crawler.rs`：`Engine` 負責單一工作、取消、頁數限制、LLM 批次與持久化。`Engine::with_components` 接受爬蟲、下載工廠及分類器。
- `src/services.rs`：`DownloadFactory`／`Fetcher` 與 `Annotator` 隔離外部 I/O；正式環境使用 `AppleDownloads` 與 `Ollama`，測試使用固定資料與可控制的失敗。

新增不同擷取策略時，實作 `Discovery`、`Extraction`、`Crawler`，再於建立 Engine 時注入。新增其他網站還需提供相應下載範圍／robots 政策與設定類別；預設下載器仍只允許 Apple 台灣，介面目前只提供 Apple 類別。

```sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
python3 -m unittest discover -s scripts -p 'test_*.py'
```

GitHub Actions 自動執行上述檢查。Rust 測試使用 HTML fixtures、臨時 SQLite 與本機隨機連接埠的 HTTP 測試伺服器，不需 Apple 網路或 Ollama。涵蓋混合／巢狀版面、原生 colspan、重試、逾時、推論中取消、頁數限制及儲存失敗。

連網驗證另存 `{job_id}-quality.json`。`discovery` 要求每個選定類別都有規格連結；`sample` 要求每類至少一個產品；`full` 要求每個候選產品都已抽取。驗證也檢查 block ID 唯一性、完整原文對應與 LLM 失敗，未達標時以非零狀態結束。`needs_review` 數量會列於報告，仍需人工確認分類。使用 `--job` 時，`--mode` 必須符合該工作的原始選項。

`target/`、`data/`、`crawler/`、`crawler_data/` 與新驗證快照由 `.gitignore` 排除；`tests/fixtures/` 與既有 `initial_run.json` 保留供參考。

### 爬取資料目錄

預設資料根目錄為 `crawler_data/apple/`，`CRAWLER_DATA_DIR` 可覆寫完整根路徑。

```text
crawler_data/apple/
  crawler.sqlite3
  runs/{job_id}/robots.txt
  iphone/{job_id}/page-0.html
  iphone/{job_id}/page-0.txt
  iphone/{job_id}/product-1/result.json
  iphone/{job_id}/product-1/llm-0-0-request.json
  mac/{job_id}/...
  ipad/{job_id}/...
  watch/{job_id}/...
  airpods/{job_id}/...
```

類別子目錄依實際下載建立；每次工作使用獨立 ID，避免覆蓋歷史資料。各產品資料夾保存 LLM 請求／回應及結果；共用 robots 檔案保存在 `runs/`。舊 `data/` 資料保留原處，不自動搬移；若需查看舊工作，可使用 `CRAWLER_DATA_DIR=data cargo run`。

若 8080 已被使用，可用 `CRAWLER_PORT=8081 cargo run` 啟動，並以 `python3 scripts/verify_live.py --base-url http://127.0.0.1:8081 --mode sample` 驗證。

驗證腳本的快照與品質報告預設寫入 `crawler_data/apple/verification/`，也可用 `--output-dir` 指定；若伺服器使用自訂 `CRAWLER_DATA_DIR`，請讓腳本使用同一環境變數或明確指定輸出目錄。
