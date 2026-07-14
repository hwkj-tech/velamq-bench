# PR-3：前端 Shell 与 Dashboard

## 目标

把 `web/` 从单页 vanilla JS（449 行 HTML + 2498 行 JS + 4022 行 CSS）升级为 **Vite + TypeScript + Vue 3 (Composition API) + Pinia** 工程化前端，落地新的六区导航、Dashboard 聚合页、统一设计 token。本 PR 不实现 Scenario Builder / Run Detail / Compare 的具体业务（占位即可），但骨架要完整、可路由、可联调 v2 API。

> 选 Vue 3：项目体量与心智模型适合 Vue 的单文件组件 + Composition API；不引 Vuetify 等组件库，自建薄组件层以保持 footprint 小。如团队偏好 React，可在评审时切换；本计划以 Vue 3 为基线。

## 前置依赖

- PR-2（v2 API + SSE multiplex）已合并；前端可直接调 `/api/v2/*`。

## 涉及文件

新增

- `web/package.json` / `web/vite.config.ts` / `web/tsconfig.json`。
- `web/index.html`（Vite 入口，简化为 `<div id="app">`）。
- `web/src/main.ts`：挂载 Vue 应用、Pinia、router、i18n。
- `web/src/App.vue`：app shell（topbar + sidebar + outlet）。
- `web/src/router/index.ts`：六区路由 + 二级路由。
- `web/src/api/client.ts`：fetch 封装、错误归一、SSE helper。
- `web/src/api/types.ts`：与后端 model 对齐的 TS 类型（`BrokerProfile / Workload / Scenario / Run / RunWorkload / Annotation / RunEvent`）。
- `web/src/stores/`：`useRuntimeStore` / `useRunsStore` / `useScenariosStore` / `useBrokersStore` / `usePayloadsStore` / `useUIStore`。
- `web/src/composables/useSse.ts`：把 EventSource 包成 `ref` reactive。
- `web/src/composables/useI18n.ts`：薄封装 `vue-i18n`。
- `web/src/theme/tokens.css`：00 设计文档里的全部 tokens（light + dark）。
- `web/src/theme/global.css`：reset + 字体 + 全局排版。
- `web/src/components/`：基础组件 `AppButton / AppCard / AppPanel / AppEmpty / AppPill / AppToast / AppDialog / AppNav / AppTabs / AppCheckbox / AppRadio / AppSelect / AppInput / AppNumberInput / AppTextarea / AppKpiCard / AppLoading / AppSparkline`。
- `web/src/pages/Dashboard.vue` / `Runs.vue` / `Compare.vue` / `Scenarios.vue` / `Templates.vue` / `SettingsLayout.vue` 与子页占位。
- `web/locales/en.json` / `zh-CN.json`：迁移老的 i18n key 并补齐新文案。

修改

- `src/main.rs` 把 `ServeDir::new(web_dir)` 改为 `ServeDir::new(web_dir.join("dist"))`，404 fallback 指向 `dist/index.html`。
- 老 `web/{index.html, app.js, styles.css}` 在 PR-3 完成后保留为 `web/legacy/` 子目录（备查），提交里带 `chore(web): archive legacy ui`。

构建产物

- `npm run build` 输出 `web/dist/`，axum 直接 serve；开发时跑 `npm run dev`，Vite proxy `/api/*` 到 `http://127.0.0.1:8088`。

## 数据 / 接口契约

### 1. 路由

```
/                       -> redirect /dashboard
/dashboard
/runs
/runs/:id               -> Run Detail（PR-5 实现，PR-3 给空架子）
/compare?ids=a,b,c
/scenarios
/scenarios/:id          -> Scenario Detail（PR-4 实现）
/scenarios/:id/run      -> Builder（PR-4 实现）
/templates
/settings/brokers
/settings/payloads
/settings/network
/settings/preferences
```

`router-link` 与 sidebar 高亮使用 `route.matched`，二级页通过 `<RouterView />` 嵌入。

### 2. State store 草图

```ts
// stores/runtime.ts
export const useRuntimeStore = defineStore('runtime', () => {
  const activeRunId = ref<string | null>(null);
  const status = ref<RunStatus>('idle');
  const lastSnapshotByWorkload = ref<Record<string, MetricSnapshot>>({});
  const sse = ref<EventSourceController | null>(null);

  function attach(runId: string) { ... }
  function detach() { ... }
  return { activeRunId, status, lastSnapshotByWorkload, attach, detach };
});

// stores/runs.ts
export const useRunsStore = defineStore('runs', () => {
  const list = ref<RunSummary[]>([]);
  const cursor = ref<string | null>(null);
  const filters = reactive({ scenarioId: '', status: '', search: '' });
  async function load(reset = false) { ... }
  async function loadOne(id: string) { ... }
  return { list, cursor, filters, load, loadOne };
});
```

### 3. SSE 接入约定

`useSse(runId)` 返回 reactive `{ status, lastSnapshot, lastLogs, annotations, replay(sinceMs) }`，背后是 `EventSource('/api/v2/runs/<id>/events?since_ms=<n>')`。所有页面共用，避免每个 chart 自起一条流。

## 实施步骤

### 1. Vite 工程化骨架

```bash
cd web
npm create vite@latest . -- --template vue-ts
npm i -D @types/node prettier eslint @typescript-eslint/parser @typescript-eslint/eslint-plugin
npm i pinia vue-router@4 vue-i18n@9
```

`vite.config.ts`：

```ts
export default defineConfig({
  base: './',
  plugins: [vue()],
  build: { outDir: 'dist', sourcemap: true },
  server: {
    proxy: { '/api': 'http://127.0.0.1:8088' },
  },
});
```

`tsconfig.json` 开启 `strict: true`、`exactOptionalPropertyTypes: true`、`noUncheckedIndexedAccess: true`。

### 2. App Shell 布局

`App.vue` 大致：

```vue
<template>
  <div class="app-shell" :data-theme="ui.theme">
    <header class="topbar">
      <BrandMark />
      <QuickBenchTrigger />
      <RuntimeStatusPill />
      <RunIdBadge v-if="runtime.activeRunId" />
      <LanguageSelect />
      <ThemeToggle />
      <HelpMenu />
    </header>
    <aside class="sidebar">
      <AppNav :items="navItems" />
    </aside>
    <main class="workspace">
      <RouterView />
    </main>
    <ToastHost />
  </div>
</template>
```

样式纯 CSS Grid：`grid-template-columns: 248px 1fr; grid-template-rows: 56px 1fr;`，topbar 跨两列，sidebar 与 workspace 各占一格。

### 3. Dashboard 页

布局严格按 `00-design-overview.md` §3.1：

```vue
<DashboardKpiStrip :data="kpis" />
<div class="dashboard-row">
  <DashboardActiveRuns :runs="active" />
  <DashboardRegressions :items="regressions" />
</div>
<DashboardRecentRuns :runs="recent" />
```

数据来源（PR-3 用 mock，PR-6 实装）：

- `kpis` = `GET /api/v2/runs?limit=50` 客户端聚合（PR-6 后端补 `/dashboard/summary`）。
- `active` = 来自 `useRuntimeStore`。
- `regressions` = 暂时空数组（PR-6 接 baseline 比较）。
- `recent` = `GET /api/v2/runs?limit=12&sort=started_at_desc`，每张卡片用 `AppSparkline` 跑最近 60 个 snapshot。

### 4. Topbar QuickBench

弹出 sheet（不是 dialog），允许两种快速发起：

1. 选已有 Scenario → 立即 `POST /scenarios/{id}/run`。
2. 填 broker 地址 + workload 模式 → 后端 `POST /api/v2/runs` ad-hoc。

弹窗组件 `QuickBenchSheet.vue`，提交后 router push 到 `/runs/:id`。

### 5. 设计 token 落地

`theme/tokens.css` 包含 `:root` 与 `:root[data-theme='dark']` 两套，按 00 文档表格落值。

`theme/global.css`：

```css
* { box-sizing: border-box; }
html, body, #app { height: 100%; }
body {
  font-family: 'Inter', 'PingFang SC', 'Microsoft YaHei', system-ui, sans-serif;
  background: var(--bg-canvas);
  color: var(--fg-default);
}
```

老 `styles.css` 不直接复用；新组件按 token 写 `.app-button { background: var(--accent-primary); ... }`。

### 6. i18n 迁移

把老 `app.js` 内 `I18N` 字典抽到 `web/locales/{en,zh-CN}.json`：

```json
// en.json
{
  "nav": { "dashboard": "Dashboard", "runs": "Runs", ... },
  "dashboard": { "kpi.published": "Published", "regressions.title": "Regressions vs Baseline", ... },
  ...
}
```

`useI18n` 内：

```ts
import { createI18n } from 'vue-i18n';
import en from '../../locales/en.json';
import zhCN from '../../locales/zh-CN.json';

export const i18n = createI18n({
  legacy: false,
  locale: localStorage.getItem('lang') ?? navigator.language.startsWith('zh') ? 'zh-CN' : 'en',
  fallbackLocale: 'en',
  messages: { en, 'zh-CN': zhCN },
  missingWarn: true,
});
```

缺 key 时显示 `[key.path]`（不再悄悄回退英文）。

### 7. 占位页

- `Runs.vue` / `Scenarios.vue` / `Templates.vue` / `Compare.vue` / `Settings*.vue`：仅展示标题 + 「Coming in PR-X」空状态卡片，避免阻塞路由。
- `Run Detail` 路由 `/runs/:id` 显示 `Overview` Tab 占位（消费 SSE，渲染 KPI 行）。

### 8. axum 改 serve dist

```rust
let dist = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web/dist");
let index = dist.join("index.html");
let static_files = ServeDir::new(dist).not_found_service(ServeFile::new(index));
```

`README.md` 增加：

```bash
cd web && npm install && npm run build   # 一次性出 dist/
cargo run                                # axum serve dist + /api/v2
# 开发模式：cd web && npm run dev (Vite proxy /api)
```

## 验证

```bash
cd web
npm run lint
npm run build
npm run typecheck

cd ..
cargo run
```

手动：

- 浏览器访问 `http://127.0.0.1:8088/`，确认 6 个一级路由可切换、URL hash 同步。
- Dashboard 拉到 12 张 recent run 卡，sparkline 渲染正确。
- Topbar QuickBench 选 ad-hoc broker 起一个 run，跳转到 `/runs/:id`，能收到 SSE。
- 切 dark theme，全局 token 正常生效（不出现"半亮半黑"）。
- 切 zh-CN，所有 `nav.*` / `dashboard.*` 文案中文化；缺 key 的位置显示 `[key]`。

## 风险与回滚

- **风险**：引入 Vite + Vue + Pinia 增加运维复杂度。缓解：`npm install` 后 `npm run build` 必须可在 CI 上 reproducible（lockfile 锁版本），并把 `web/dist/` 不入库（仅由 CI 构建后随 release 打包）。
- **风险**：旧浏览器（IE / 老 Edge）不再支持。缓解：原本就只测 Chromium / Firefox / Safari 近 2 个大版本，文档明确写出。
- **回滚**：保留 `web/legacy/`，axum 在 fallback 阶段优先 serve `web/dist`，缺失时退回 `web/legacy/`；只需切换 `ServeDir` 路径即可灰度。
