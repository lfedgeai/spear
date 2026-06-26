# 前端改造路线图

## 目标

让 `spear` 前端代码更符合业界 best practice：

- 更模块化
- 更优雅
- 更容易维护
- 更便于开发者阅读
- 尽量避免过长函数和超大页面组件

本路线图优先处理真正影响维护成本的热点文件，而不是先做零碎样式或小工具函数清理。

## 范围

本次改造聚焦前端源码，不处理构建产物：

- 处理：
  - `web-console/src/*`
  - `web-admin/src/*`
- 不处理：
  - `assets/admin/*`
  - `assets/console/*`

## 优先级

### P0. Web Console 拆大容器

目标文件：

- `web-console/src/App.tsx`

当前问题：

- 页面容器同时承担会话状态、WebSocket 生命周期、语音录制生命周期、数据拉取、弹窗状态、消息格式化、localStorage 持久化和 JSX 编排
- 单文件过大，阅读成本高，局部修改容易牵一发动全身

拆分原则：

- 先拆状态与副作用，再拆 UI 结构组件
- 不急着引入全局状态库
- 不先做大量零碎 helper 文件

计划步骤：

1. 提取 `useConversationStore`
2. 提取 `useSpearConnection`
3. 提取 `useVoiceSession`
4. 提取 `useConnectDrawerModel`
5. 再拆 `ConversationSidebar`、`ChatHeader`、`ChatComposer`、`ChatBubble`

### P1. Web Admin 拆任务页

目标文件：

- `web-admin/src/features/tasks/TasksPage.tsx`

当前问题：

- 一个页面里同时包含任务列表、创建任务大表单、文件选择、MCP 权限配置、payload 组装、创建后立即运行
- 实际上已经是“页面 + 子系统”混合体

计划步骤：

1. 先把 `CreateTaskDialog` 独立成单文件
2. 提取 `buildCreateTaskPayload`
3. 提取任务运行 action / hook
4. 再拆表单分区组件

### P2. Web Admin 拆 AI Models 双模式容器

目标文件：

- `web-admin/src/features/ai-models/AiModelsPage.tsx`

当前问题：

- 一个页面同时承载 local / remote 两条业务线
- 路由已经区分了两类页面，但容器层仍然过厚

计划步骤：

1. 拆成 `LocalAiModelsPage`
2. 拆成 `RemoteAiModelsPage`
3. 下沉本地删除 deployment 的业务编排

### P3. Web Admin 收敛实例运维共性

目标文件：

- `web-admin/src/features/tasks/TaskDetailPage.tsx`
- `web-admin/src/features/instances/InstanceDetailPage.tsx`

当前问题：

- 实例销毁流和弹窗逻辑重复
- 页面既做详情视图又做运维动作编排

计划步骤：

1. 提取 `DestroyInstanceDialog`
2. 提取 `useDestroyInstance`
3. 再拆 overview / instances card

## 实施原则

### 先拆什么

- 状态管理
- 副作用
- 页面级业务编排

### 后拆什么

- 纯工具函数
- 轻量 UI 组件
- 样式细节

### 暂不建议

- 现在就引入 Redux / Zustand 这类全局状态库
- 现在就做通用 CRUD 框架
- 为了“统一”而过度抽象页面业务差异

## 当前进度

### Step 1 已开始并完成

已完成：

- 新增 `web-console/src/models/conversation.ts`
- 新增 `web-console/src/hooks/useConversationStore.ts`
- 将 `App.tsx` 中的会话模型、localStorage 持久化、active 会话切换、基础会话 CRUD 收口进 `useConversationStore`

这一刀的目标不是一次性把 `App.tsx` 全拆完，而是先把“会话状态层”从页面中剥离出来，为后续拆连接、语音和抽屉模型打基础。

### 下一步

下一刀按本路线图继续推进：

- 提取 `useSpearConnection`

目标：

- 把 WebSocket client map、连接状态推进、自动连接逻辑、文本发送逻辑从 `App.tsx` 中继续剥离
- 让页面容器更接近“状态编排 + 组件组装”
