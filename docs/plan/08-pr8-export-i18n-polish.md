# PR-8：导出包 / i18n 全覆盖 / a11y 与收尾

## 目标

收尾 PR：

1. **Bench Bundle**：把 run + scenario + 全量 snapshots + annotations + 引用 profile 打成单 JSON / zip，导入后可在另一台机器复现 Compare / Baseline。
2. **i18n 全覆盖**：所有页面、空状态、错误 toast、a11y label 走 `useI18n`；缺 key 检测 CI 化。
3. **可访问性 / 键盘 / 主题**：完整一遍 axe + tab order；明暗主题自动跟随 + 手动覆盖。
4. **错误状态 / 空状态 / 加载状态**：抽 `AppEmpty / AppError / AppLoading` 一致化。
5. **下线旧 UI**：`web/legacy/` 移除；旧 `/api/bench/*` 加 `Deprecation` header；老表 `bench_specimens` 进入只读。
6. **文档**：更新 `README.md` / `CHANGELOG.md` / `docs/api.md`。

## 前置依赖

- PR-1 ~ PR-7 全部合并。

## 涉及文件

新增

- `src/api/bundle.rs`：`POST /api/v2/bundles/export`、`POST /api/v2/bundles/import`。
- `src/runtime/bundle.rs`：bundle 序列化/反序列化、id 重映射。
- `web/src/pages/RunDetailExportSheet.vue`：Run Hero 内"Export"菜单挂的 Bundle 选项 UI。
- `web/src/pages/settings/Import.vue`：Settings 下 `Import Bundle` 入口（仅本期 PR）。
- `web/src/components/feedback/AppEmpty.vue` / `AppError.vue` / `AppLoading.vue` / `AppToast.vue` / `AppSkeleton.vue`。
- `web/src/composables/useTheme.ts`：跟随 `prefers-color-scheme` + localStorage 覆盖。
- `web/scripts/check-i18n.ts`：扫源码所有 `t('xxx')`，断言 `en` / `zh-CN` JSON 都存在；missing key 失败。
- `web/scripts/check-a11y.ts`：用 `pa11y-ci` 跑主要路由 smoke test（可在本地）。
- `docs/api.md`：v2 REST 端点一览。

修改

- 所有页面 / 组件最后一遍清扫硬编码英文 → i18n key。
- `src/main.rs`：旧 `/api/bench/*` 路由加 middleware `add_deprecation_header()`。
- `src/storage.rs`：`bench_specimens` / `bench_templates` 转为只读视图（`CREATE VIEW` 替换 `CREATE TABLE`，原表数据通过 1 次性 dump 到 archived 文件 `data/legacy_dump.json` 后保留备份）。
- `web/legacy/` 删除。`web/index.html` Vite 入口最终版。
- `README.md`：构建步骤、bundle 用法、i18n 贡献指南。
- `package.json`：`scripts` 添加 `lint:i18n` / `lint:a11y`。

## 数据 / 接口契约

### 1. Bundle 格式

```jsonc
{
  "version": "1.0",
  "generated_at": "2026-05-04T12:00:00Z",
  "source": { "host": "ws-01", "velamq_bench": "0.2.0" },
  "scenarios": [ { ... } ],
  "broker_profiles":  [ { ... } ],
  "payload_profiles": [ { ... } ],
  "runs": [
    {
      "run":       { ... },
      "workloads": [ { ... } ],
      "snapshots": [ { ... } ],   // 全量 snapshots
      "annotations": [ { ... } ],
      "regression": { ... }       // 可选
    }
  ]
}
```

支持 zip 包：`bundle.json` + `csv-samples/...`（CsvReplay payload 的样本数据），后端校验 `csv_replay.path` 是否在 zip 内的 `csv-samples/` 下并改写路径。

### 2. Export endpoint

```
POST /api/v2/bundles/export
body: { "run_ids": ["r1","r2"], "include_snapshots": true, "format": "json" | "zip" }
resp: 200 application/json|application/zip + Content-Disposition
```

无 `run_ids` 时导出所有"近 N 条 run"或"全部 scenario"——本期硬性要求 `run_ids` 不为空。

### 3. Import endpoint

```
POST /api/v2/bundles/import (multipart)
fields:
  bundle: file (json or zip)
  conflict: "skip" | "rename" | "overwrite"
resp: { "imported": { "scenarios": 2, "runs": 5, "broker_profiles": 1, "payload_profiles": 2 } }
```

id 处理：

- `scenarios.id` / `runs.id` 不可冲突；遇到冲突按 `conflict` 策略。
- `broker_profiles.id` / `payload_profiles.id`：默认 `rename`（自动加后缀 `-imported-{ts}`），保证多次导入幂等地新增。
- 所有引用通过新旧 id 映射表 `HashMap<old_id, new_id>` 重写。

## 实施步骤

### 1. Bundle 导出

`src/runtime/bundle.rs::export(run_ids: &[String]) -> Result<Bundle>`：

```rust
let runs = run_repo.get_many(&run_ids)?;
let scenarios = scenario_repo.get_many_by_run(&run_ids)?;
let broker_ids = collect_broker_ids(&runs, &scenarios);
let payload_ids = collect_payload_ids(&runs, &scenarios);
let snapshots = run_repo.snapshots_full(&run_ids)?;
let annotations = run_repo.annotations(&run_ids)?;
Ok(Bundle {
    version: "1.0",
    runs, scenarios,
    broker_profiles: broker_repo.get_many(&broker_ids)?,
    payload_profiles: payload_repo.get_many(&payload_ids)?,
    snapshots, annotations,
    ...
})
```

序列化优先 JSON；zip 走 `zip` crate（新增依赖）。

### 2. Bundle 导入

事务一致性：所有插入在同一个 transaction 内，遇到任意校验失败整体回滚。

```rust
let tx = conn.transaction()?;
let mut id_map = IdMap::default();
for profile in bundle.broker_profiles { upsert_with_remap(&tx, &mut id_map, profile, conflict)?; }
for profile in bundle.payload_profiles { ... }
for scenario in bundle.scenarios       { ... }
for run_pkg in bundle.runs             { ... }
tx.commit()?;
```

`upsert_with_remap`：

- `conflict="skip"`：已存在则跳过。
- `conflict="rename"`：生成新 id（uuid v4）+ 名字加后缀 `({yyyy-mm-dd})`。
- `conflict="overwrite"`：直接覆盖（不动 history）。

### 3. UI 入口

Run Detail Hero 的 Export 菜单加：

```
[ Export ▾ ]
   • JSON snapshot
   • CSV
   • SVG image
   • PDF report
   • Bundle (.zip) — full reproducible
```

Settings 增加 `Import` 子页：上传文件 → 显示预览（包含数量 / scenarios 名单）→ 选 conflict 策略 → 提交 → 完成 toast 跳到 `/runs?ids=...`。

### 4. i18n 全覆盖

- 写脚本 `web/scripts/check-i18n.ts`：用 `babel-parser` 扫所有 `.vue` 与 `.ts`，提取 `t('xxx')` / `$t('xxx')` 调用，对比 `web/locales/{en,zh-CN}.json`，缺失抛错。
- `package.json` 加 `"lint:i18n": "tsx scripts/check-i18n.ts"`，CI 跑。
- 把 PR-3 ~ PR-7 中遗漏的硬编码字符串清扫（grep `>[A-Z][a-z ]+<` 与 `placeholder="..."`）。

### 5. a11y 走查

- `web/src/components/AppButton.vue` 等基础组件统一带 `aria-busy`、`aria-disabled` 状态。
- 所有图表图例 + Tab 都用 `role="tablist" / role="tab"`，键盘 `← → Home End` 可达。
- 颜色对比：每个 token 在 light / dark 上对前景文本至少 4.5:1（用 `polished` 或 `culori` 在 build 时核验）。
- `pa11y-ci` smoke：Dashboard / Runs / RunDetail/Overview / Compare / Settings/Brokers，allow list 几个不可避免的告警。

### 6. 错误 / 空 / 加载状态

- 所有列表：使用 `<AppEmpty :title="..." :hint="..." :action="..." />`，不再自渲染。
- 所有 fetch 链路：`AppLoading.vue`（带骨架色块）；失败时 `AppError.vue`（带 retry 按钮）。
- Toast：`AppToast.vue` + `useToast()` 全局单例；最多同屏 3 条。

### 7. 旧 UI / 旧 API 下线

- `web/legacy/` 删除（git 提交单独提交方便回看）。
- `src/main.rs` 给 `/api/bench/*` 加 middleware：

  ```rust
  async fn legacy_deprecation_layer(req: Request, next: Next) -> Response {
      let mut resp = next.run(req).await;
      resp.headers_mut().insert(
          "Deprecation", "true".parse().unwrap(),
      );
      resp.headers_mut().insert(
          "Sunset", "Sat, 01 Aug 2026 00:00:00 GMT".parse().unwrap(),
      );
      resp.headers_mut().insert(
          "Link", "</api/v2/runs>; rel=\"successor-version\"".parse().unwrap(),
      );
      resp
  }
  ```

- 在 README 标注旧 API 退役计划（至少保留一个版本）。

### 8. 文档

`README.md` 重写：

```
1. 简介
2. 一键启动（cargo run）
3. 开发：cd web && npm run dev
4. 配置（VELAMQ_BIND / data dir）
5. 概念（Scenario / Workload / Profile / Run）
6. Bundle 导入导出
7. v2 API 速查 → docs/api.md
8. i18n 贡献指南
9. 老 API 弃用说明
```

`CHANGELOG.md` 在 0.2.0 章节列出 PR-1 ~ PR-8 关键变更。

## 验证

```bash
cargo test
cargo run
cd web
npm run lint
npm run lint:i18n
npm run typecheck
npm run build
npm run lint:a11y
```

手动：

- 在机器 A 跑 3 个 run（含 1 个 baseline + 2 个对比）→ Run Detail 选 3 条 → Export Bundle → 下载 zip。
- 在机器 B 全新初始化，Settings → Import 上传 zip → 跳转 Compare → 三条 run 全部可见、KPI delta 与机器 A 一致。
- 切 zh-CN 全站走一遍：Dashboard / Runs / RunDetail 8 Tab / Compare / Scenarios / Settings → 所有按钮、空状态、错误 toast 都是中文，无英文残留。
- 仅键盘走完一次「新建 scenario → 启动 → 看图表 → mark baseline → compare」全流程。
- 旧 `curl /api/bench/start` 仍可工作，response 头部带 `Deprecation: true`。

## 风险与回滚

- **风险**：bundle 导入 id 冲突逻辑出错导致脏数据。缓解：所有插入在事务内，失败回滚；导入完成 toast 给"已导入 X scenario / Y run"，留 7 天 audit log（即使本 PR 不做 audit 表，至少 stdout log）。
- **风险**：i18n CI 严卡导致后续 PR 阻塞。缓解：`check-i18n` 默认警告级，超过 N 条才失败；项目根 `i18n.config.json` 控制阈值。
- **风险**：a11y 改造涉及大量 markup 调整影响视觉。缓解：所有改动跑视觉回归（Playwright snapshot）确保无意外位移。
- **回滚**：bundle endpoint 下线即可（不影响主流程）；i18n CI 关闭；旧 UI 通过 git revert 恢复 `web/legacy/`。
