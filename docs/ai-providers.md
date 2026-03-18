# AI Providers (Phase 1 Scaffold)

LaunchPad 目前提供了 AI 适配层基础结构，目标是支持多 provider 并保持统一调用模型。

## Provider 抽象

- `src/adapter/ai/provider.rs`
  - `AiProvider` trait
  - `AiEditRequest` / `AiEditResponse`
  - `AiStreamResponse`（stream-ready 响应容器，当前默认将 summary/notes 切分为 chunk）

## 已接入的 Provider 形态

1. **Claude Agent Sidecar**
   - `src/adapter/ai/claude_agent_sidecar.rs`
   - 通过环境变量 `LAUNCHPAD_CLAUDE_SIDECAR` 指定可执行文件路径
   - LaunchPad 会把当前 XML 快照通过 stdin 传入 sidecar，并读取 stdout 结果

2. **OpenAI-compatible / Anthropic / Google**
   - 已接入基础 HTTP 调用（`openai_compat.rs` / `anthropic.rs` / `google.rs`）
   - 当缺少 API Key 或 endpoint/model 配置不正确时，会返回明确错误提示
   - OpenAI-compatible 预置名称：`openai` / `openrouter` / `lm-studio` / `ollama` / `xai`

3. **本地启发式 fallback**
   - `AiService` 默认 fallback 到 `heuristic-local` provider
   - 可基于自然语言关键词给出 launchd 参数建议（RunAtLoad/KeepAlive/StartInterval 等）

## UI 集成

- 在 Plist Editor 区域提供：
  - prompt 输入
  - provider 循环切换按钮
  - AI Analyze 触发按钮
  - 建议文本输出面板
  - stream preview（当前为非阻塞分段展示，后续可替换为真实 token 流）

## 配置

- `LAUNCHPAD_AI_PROVIDER`
  - 指定默认 provider（如 `heuristic-local`、`openai`、`claude-sidecar`）
- `LAUNCHPAD_CLAUDE_SIDECAR`
  - 可选，指定 sidecar 可执行路径
- `LAUNCHPAD_<PROVIDER>_BASE_URL`
  - 可选，覆盖 provider 默认 endpoint（如 `LAUNCHPAD_OPENROUTER_BASE_URL`）
- `LAUNCHPAD_<PROVIDER>_MODEL`
  - 可选，覆盖默认模型名
- `LAUNCHPAD_<PROVIDER>_API_KEY`
  - provider key（openai/openrouter/lm-studio/ollama/xai/anthropic/google）

## 安全策略（当前阶段）

- AI 会先生成 **patch 预览 diff**，并要求用户点击 `Apply AI Patch` + `Confirm Apply` 才会写入编辑器状态。
- AI 不会直接落盘到 plist 文件；所有磁盘写入仍需用户显式点击 Save。
