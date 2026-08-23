# 顶层 Makefile 中禁用 spear-next

## 概述 / Overview

本文档记录了如何在顶层 Makefile 中禁用 spear-next 子项目的构建过程。

## 问题背景 / Background

顶层 Makefile 使用自动发现机制来查找所有包含 Makefile 的子目录，并将它们包含在构建过程中。这可能导致 spear-next 项目被意外包含在主项目的构建流程中。

## 解决方案 / Solution

### 修改前 / Before
```makefile
SUBDIRS := $(shell find $(REPO_ROOT) -mindepth 1 -maxdepth 3 -type d -exec test -e {}/Makefile \; -exec echo {} \;)
```

### 修改后 / After
```makefile
SUBDIRS := $(shell find $(REPO_ROOT) -mindepth 1 -maxdepth 3 -type d -exec test -e {}/Makefile \; -exec echo {} \; | grep -v spear-next)
```

## 变更说明 / Changes

1. **过滤机制** / **Filtering Mechanism**: 在 `SUBDIRS` 变量定义中添加了 `| grep -v spear-next` 过滤器
2. **排除目录** / **Excluded Directory**: `spear-next` 目录现在被排除在自动构建流程之外
3. **保持兼容** / **Compatibility**: 其他子项目的构建流程保持不变

## 影响的目标 / Affected Targets

以下 Makefile 目标不再包含 spear-next：

- `clean`: 清理操作不再包含 spear-next
- `build`: 构建操作不再包含 spear-next  
- `test`: 测试操作不再包含 spear-next

## 验证方法 / Verification

### 检查当前包含的子目录 / Check Current Subdirectories
```bash
find . -mindepth 1 -maxdepth 3 -type d -exec test -e {}/Makefile \; -exec echo {} \; | grep -v spear-next
```

### 预览构建流程 / Preview Build Process
```bash
make -n all
```

## 注意事项 / Notes

1. **独立构建** / **Independent Build**: spear-next 仍可以独立构建，只需进入其目录执行 `make`
2. **CI/CD 影响** / **CI/CD Impact**: 如果 CI/CD 流程依赖顶层 Makefile，需要相应调整
3. **恢复方法** / **Restoration**: 如需重新启用，只需移除 `| grep -v spear-next` 过滤器

## 相关文件 / Related Files

- `/Makefile` - 顶层 Makefile 文件
- `/spear-next/Makefile` - spear-next 项目的 Makefile（仍然存在且可用）

## 日期 / Date

修改日期：2024年1月
