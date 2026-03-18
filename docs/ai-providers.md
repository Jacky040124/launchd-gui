# AI Providers (Phase 1 Scaffold)

LaunchPad 目前提供了 AI 适配层基础结构，目标是支持多 provider 并保持统一调用模型。

## Provider 抽象

- `src/adapter/ai/provider.rs`
  - `AiProvider` trait
  - `AiEditRequest` / `AiEditResponse`

## 已接入的 Provider 形态

1. **Claude Agent Sidecar**
   - `src/adapter/ai/claude_agent_sidecar.rs`
   - 通过环境变量 `LAUNCHPAD_CLAUDE_SIDECAR` 指定可执行文件路径
   - LaunchPad 会把当前 XML 快照通过 stdin 传入 sidecar，并读取 stdout 结果

2. **OpenAI-compatible / Anthropic / Google**
   - 已有模块骨架（`openai_compat.rs` / `anthropic.rs` / `google.rs`）
   - 当前构建仍返回 “not configured” 提示，后续阶段会接入真实 API 调用

3. **本地启发式 fallback**
   - `AiService` 默认 fallback 到 `heuristic-local` provider
   - 可基于自然语言关键词给出 launchd 参数建议（RunAtLoad/KeepAlive/StartInterval 等）

## UI 集成

- 在 Plist Editor 区域提供：
  - prompt 输入
  - AI Analyze 触发按钮
  - 建议文本输出面板

## 安全策略（当前阶段）

- AI 仅生成建议文本，不会自动写回 plist。
- 所有实际修改仍由用户点击 Save 执行。
