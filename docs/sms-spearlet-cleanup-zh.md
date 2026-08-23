# SMS 与 Spearlet 清理说明

## 范围

本次清理针对主 Rust workspace 中两个低风险、以可读性为主的点：

- `src/sms/services/resource_service.rs`
- `src/spearlet/object_service.rs`

目标是在不改变对外行为的前提下，移除无效暴露面并减少内部重复逻辑。

## 变更

- 从 `resource_service.rs` 中移除了 3 个未被使用的 `NodeResourceInfo` helper：
  - `update_metadata`
  - `get_memory_usage_bytes`
  - `get_available_disk_bytes`
- 在 `object_service.rs` 中集中处理 Unix 时间戳生成，避免对象写路径重复同一段时间计算逻辑。
- 将对象统计聚合收敛到单一内部扫描 helper，减少以下方法中的重复扫描与反序列化逻辑：
  - `object_count`
  - `total_object_size`
  - `pinned_object_count`
  - `get_stats`
- 在 `object_service.rs` 中补上共享的对象加载/保存 helper，让 `put/get/引用计数/pin/unpin/delete` 不再重复 KV 读取、反序列化、序列化和回写样板代码。
- 补充了一个聚焦测试，验证 `get_stats()` 能正确统计对象总大小和 pinned 对象数量。
- 补充了一个聚焦的 overwrite 测试，锁定现有行为：覆盖写入只更新载荷相关字段，不破坏引用计数和 pinned 状态。

## 验证

- `cargo test resource_service --lib`
- `cargo test object_service --lib -- --nocapture`

`cargo clippy --lib -- -D warnings` 仍会报出工作区内其他未改动文件的既有 warning，因此本次以受影响模块的定向测试作为主要验证方式。
