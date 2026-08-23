# Documentation Directory

This directory contains project documentation, recording important changes, solutions, and configuration instructions during project development.

## Start Here

If this is your first time using SPEAR, read these documents first:

- **[../README.md](../README.md)** - Repository-level overview, architecture, and Docker Compose HTTP / HTTPS quick start
- **[project-architecture-overview-en.md](./project-architecture-overview-en.md)** - Higher-level architecture explanation
- **[web-admin-overview-en.md](./web-admin-overview-en.md)** - What you can do in Web Admin
- **[spear-console-overview-en.md](./spear-console-overview-en.md)** - What Console is for
- **[samples-build-guide-en.md](./samples-build-guide-en.md)** - How to build and try samples

## Document List

### Current Documentation
- **[ai-backend-unified-control-plane-design-en.md](./ai-backend-unified-control-plane-design-en.md)** - Unified AI backend control-plane redesign covering UUID identity, placements, node status, and the read-only AI Models view
- **[ai-backend-unified-control-plane-design-zh.md](./ai-backend-unified-control-plane-design-zh.md)** - Chinese version of the unified AI backend control-plane redesign
- **[remote-ai-backend-preflight-design-en.md](./remote-ai-backend-preflight-design-en.md)** - Remote AI backend preflight design covering credential checks, model access validation, and `all_nodes` verification policy
- **[remote-ai-backend-preflight-design-zh.md](./remote-ai-backend-preflight-design-zh.md)** - Chinese version of the remote AI backend preflight design
- **[repo-type-model-refactor-roadmap-en.md](./repo-type-model-refactor-roadmap-en.md)** - Repository-wide roadmap for replacing weak typed bags, magic strings, and flat multi-semantic DTOs with stronger typed models
- **[repo-type-model-refactor-roadmap-zh.md](./repo-type-model-refactor-roadmap-zh.md)** - Chinese version of the repository type-model refactor roadmap
- **[web-admin-overview-en.md](./web-admin-overview-en.md)** - Current Web Admin page / API overview
- **[web-admin-overview-zh.md](./web-admin-overview-zh.md)** - 中文版 Web Admin 页面与 API 概览
- **[web-admin-ui-guide-en.md](./web-admin-ui-guide-en.md)** - Web Admin usage guide with current AI backend and task flows
- **[web-admin-ui-guide-zh.md](./web-admin-ui-guide-zh.md)** - 中文版 Web Admin 使用指南
- **[backend-support-matrix-en.md](./backend-support-matrix-en.md)** - Current backend adapter support matrix
- **[backend-support-matrix-zh.md](./backend-support-matrix-zh.md)** - 当前 backend 能力矩阵（中文）

### Project Structure Changes
- **[spear-next-library-conversion-zh.md](./spear-next-library-conversion-zh.md)** - Detailed record of spear-next project conversion to pure library (Chinese)
- **[spear-next-library-conversion-en.md](./spear-next-library-conversion-en.md)** - spear-next Project Conversion to Pure Library

## Documentation Guidelines

### Naming Convention
- Chinese documents end with `-zh.md`
- English documents end with `-en.md`
- Document names use lowercase letters and hyphens as separators

### Content Scope
This directory records the following types of documentation:
1. **Project Structure Changes** - Important code structure adjustments and refactoring
2. **Configuration Modifications** - Build configuration, dependency management changes
3. **Problem Solutions** - Issues encountered during development and their solutions
4. **Best Practices** - Experience and standards summarized during project development

### Purpose
- Provide project history and change records for future developers
- Help other AI tools understand project structure and design decisions
- Serve as an important carrier for project knowledge transfer

## Update Log
- 2024-01-XX: Created docs directory and index files
- 2024-01-XX: Added spear-next library project conversion documentation
- 2026-07-05: Added unified AI backend control-plane design documents
- 2026-08-16: Updated Web Admin / AI backend docs to match current code and UI flows
- 2026-08-16: Split the docs index into current vs historical sections
- 2026-08-21: Added remote AI backend preflight design documents
- 2026-08-21: Added repository type-model refactor roadmap documents
