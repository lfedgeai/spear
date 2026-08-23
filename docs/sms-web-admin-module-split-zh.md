# SMS Web Admin 模块拆分说明

## 范围

本次清理进行了一次文件级模块化拆分，涉及：

- `src/sms/web_admin.rs`
- `src/sms/web_admin/ai_backend_admin.rs`

## 架构调整

前几轮清理虽然已经把共享响应 mapper 抽了出来，但 `web_admin.rs` 里仍然直接承载了完整的 AI backend 管理实现，包括：

- 请求 body 结构
- query 结构
- parse/build helper
- handler 函数

这意味着即使响应映射已经被清理，主 Web Admin 文件依然背着一整块边界清晰但体量较大的功能区。

本次清理把 AI backend 管理功能真正移动到了独立模块：

- `web_admin/ai_backend_admin.rs` 现在统一承接 AI backend 的请求结构、输入解析 helper、proto 构造 helper 和 handler
- `web_admin.rs` 通过 `pub(crate) use` 暴露 router 需要的模块项
- 主文件进一步回到集成/组合入口的角色，而不是继续充当“大型实现仓库”

## 收益

- 进一步降低了 `web_admin.rs` 的体积和认知负担
- 让 AI backend 管理逻辑更容易被独立定位和演进
- 通过把请求类型、handler 和输入归一化放到一起，模块边界更清晰
- 让代码从“只抽 mapper”继续推进到真正的文件级模块化

## 验证

- `cargo test web_admin --lib`
- `cargo test handlers_test --lib`
- 已对以下文件执行定向诊断：
  - `src/sms/web_admin.rs`
  - `src/sms/web_admin/ai_backend_admin.rs`
