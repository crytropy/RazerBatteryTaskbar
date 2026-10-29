# RazerBatteryTaskbar

在 Windows 系統匣顯示 Razer 無線裝置電量的小工具。

這個分支正在逐步現代化。原本的 Electron/USB 版本仍然是**可正常使用的相容版**，同時平行開發一個輕量的 Rust 原生版本，並在真實硬體上驗證。

## 我該用哪一版？

### 相容版（Compatibility build）：一般測試請用這個

Electron 版是目前預期能讀到真實裝置電量的版本。

下載最新的 Windows 自動安裝檔：

https://github.com/crytropy/RazerBatteryTaskbar/releases/tag/dev-build

檔案：

`RazerBatteryTaskbar-Windows-Setup.exe`

只要 `main` 上與相容版執行環境相關的程式碼有變動，這個開發版就會自動重新建置。

### 原生預覽版（Native Preview）：架構與 UI 測試用

Rust 原生系統匣已經能以輕量 Windows 系統匣程式的形式執行，包含：

- 原生系統匣圖示與選單；
- 多裝置核心／狀態模型；
- 持久化設定；
- 執行期可調整的輪詢間隔；
- 低電量／嚴重低電量門檻；
- Windows 電量通知；
- 開機自動啟動；
- 單一執行個體保護；
- 不含隱私資訊的前端快照；
- 唯讀的本機整合 API；
- 可選的 Seelen UI 轉接器。

**預設啟動即使用原生 Windows HID 傳輸層（`windows-hid`），不需要任何參數。** 目前支援的硬體為 DeathAdder V3 Pro（接收器 1532:00B7 與有線模式 1532:00B6 視為同一隻裝置，#18）；其他硬體尚未啟用。在這顆接收器上，持續輪詢、拔插／睡眠喚醒後的自動重連、以及滑鼠休眠時的處理（保留上次電量）都已經過數小時的實機驗證；與 Synapse 共存則尚未驗證。沒有接上接收器時程式仍會正常啟動，托盤顯示未偵測到裝置。

解壓縮原生預覽版後，先關閉 Synapse 與 Razer 相關服務，再直接執行：

```powershell
.\RazerBatteryTaskbar-Native-Preview.exe
```

程式會在啟動時送出一次電量查詢，之後每次查詢至少間隔 60 秒（輪詢間隔選單只提供 1 分鐘與 5 分鐘，舊設定檔中較短的間隔會在啟動時提高到 60 秒）。舊的 `--experimental-hid-00b7` 參數仍可接受，但只是 no-op；`--placeholder-transport` 則是沒有硬體時用來測試介面的除錯選項。使用前請先閱讀 [docs/NATIVE_TRAY.md](docs/NATIVE_TRAY.md)。

最新預覽版可從 **Rust Native Preview** 工作流程的 artifact 取得：

https://github.com/crytropy/RazerBatteryTaskbar/actions/workflows/rust-core.yml

Artifact：

`RazerBatteryTaskbar-Native-Preview`

內容包含：

- `RazerBatteryTaskbar-Native-Preview.exe`
- `SHA256SUMS.txt`
- `README-NATIVE-PREVIEW.txt`

### HID Probe：硬體驗證工具

同一個 Rust 工作流程也會產出：

`RazerBatteryTaskbar-HID-Probe`

這是一個唯讀的診斷工具，用來在啟用原生電量傳輸層之前列舉 Razer 的 HID collection。它不會開啟裝置控制代碼，也不會送出 Razer 電量協定請求。

解壓縮 HID Probe 的 ZIP 後，在 PowerShell 執行：

```powershell
.\RazerBatteryTaskbar-HID-Probe.exe
.\RazerBatteryTaskbar-HID-Probe.exe --json
.\RazerBatteryTaskbar-HID-Probe.exe --pid 0x00AB
```

JSON 報告不包含序號與 HID 路徑。產品 ID 在資料庫中有紀錄，不代表它的 HID 電量協定已經驗證過。詳見 [docs/TESTING.md](docs/TESTING.md)。

## 架構

```text
Razer 裝置
    │
    ├─ Electron 相容版執行環境 ── Windows 系統匣
    │
    └─ 原生傳輸層（硬體驗證進行中）
             │
             ▼
        Rust Core
             │
      ┌──────┼────────┐
      ▼      ▼        ▼
   系統匣   本機 API   未來的轉接器
             │
             ▼
        Seelen UI
```

架構遵守四條規則：

1. Rust Core 不依賴任何 UI 框架。
2. UI 整合不直接操作 USB／HID 裝置。
3. Windows 系統匣不解析 Razer 協定。
4. 像 Seelen UI 這類可選整合，拿掉後不影響核心程式。

遷移進度請見 [ROADMAP.md](ROADMAP.md)。

## 本機整合 API

原生預覽版提供一個唯讀的 loopback 端點：

```text
http://127.0.0.1:27212/v1/status
```

這份有版本號的 JSON 合約包含裝置名稱／類型、產品與廠商 ID、電量、充電狀態、連線狀態，以及傳輸層健康狀態。

它刻意**不**包含：

- USB 序號；
- 內部裝置 ID；
- HID 路徑；
- 介面編號；
- 原始 Razer 協定資料。

此端點只接受 GET，不提供 HID 或設定控制指令。

另有一個內容相同、但以 `text/plain` 回應的版本：

```text
http://127.0.0.1:27212/v1/status.txt
```

這是給 Seelen UI 轉接器用的（Seelen 2.8.x 無法在外掛腳本中讀取 JSON 格式的遠端資料），其他整合請繼續使用 `/v1/status`。

詳見 [docs/INTEGRATION.md](docs/INTEGRATION.md)。

## Seelen UI

Seelen UI 支援是**可選的轉接器**，不屬於核心架構。

Fancy Toolbar 整合位於：

```text
integrations/seelen-ui/
```

它只透過 Seelen Fancy Toolbar 的 `remoteData` 讀取本機狀態 API，在工具列顯示電池圖示與百分比（50% 以上白色、50% 以下橘色、20% 以下紅色），滑鼠休眠時顯示上次電量並加上 💤。

開發時載入：

```powershell
slu resource load plugin .\integrations\seelen-ui
```

卸載：

```powershell
slu resource unload plugin .\integrations\seelen-ui
```

永久安裝方式與 Seelen 沙盒的注意事項請見 [integrations/seelen-ui/README.md](integrations/seelen-ui/README.md)。

## 從原始碼執行相容版

需求：

- Windows
- Node.js 18
- npm

```powershell
git clone https://github.com/crytropy/RazerBatteryTaskbar.git
cd RazerBatteryTaskbar
npm ci
npm start
```

測試：

```powershell
npm test
```

建置 Electron 的 Windows 安裝檔：

```powershell
npm run make
```

## 建置 Rust 工作區

安裝 stable 版 Rust 工具鏈（Windows 上另需 Visual Studio Build Tools 的 C++ 元件），然後：

```powershell
cargo test --workspace
cargo build -p razer-tray --release
```

原生系統匣執行檔會產生在：

```text
target/release/razer-tray.exe
```

建置唯讀的 HID 列舉探測工具：

```powershell
cargo build -p razer-core --example hid-probe --features windows-hid --release
```

詳見 [docs/RUST_CORE.md](docs/RUST_CORE.md)、[docs/NATIVE_TRAY.md](docs/NATIVE_TRAY.md) 與 [docs/TESTING.md](docs/TESTING.md)。

## 裝置資料庫

支援／已知的 Razer USB 產品 ID 與協定 transaction ID 只有一個共用的資料來源：

```text
src/devices/razer-products.json
```

資料庫目前涵蓋以下系列的裝置／變體：

- Basilisk X / Ultimate / V3 Pro / V3 X HyperSpeed
- DeathAdder V2 / V3 Pro
- Viper Ultimate / V2 Pro
- Naga V2 Pro
- Cobra Pro
- Mouse Dock Pro
- HyperPolling Wireless Dongle
- BlackShark V2 Pro（2020 與 2023 版）

實際相容性仍取決於特定硬體／韌體組合所使用的傳輸層與介面。

## 致謝

原始專案與協定實作建立在以下成果之上：

- [OpenRazer](https://github.com/openrazer/openrazer)
- [hsutungyu/razer-mouse-battery-windows](https://github.com/hsutungyu/razer-mouse-battery-windows)

原始專案作者：Tyler Dougherty。

授權：ISC。
