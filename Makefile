# SPEAR Next Makefile / SPEAR Next构建文件
# This Makefile provides targets for building, testing, and analyzing the SPEAR Next project
# 此Makefile为SPEAR Next项目提供构建、测试和分析目标

# Project configuration / 项目配置
PROJECT_NAME := spear-next
VERSION := $(shell git describe --tags --match "*" --always --dirty 2>/dev/null || echo "dev")
REPO_ROOT := $(shell pwd)
TARGET_DIR := $(REPO_ROOT)/target
COVERAGE_DIR := $(TARGET_DIR)/coverage
COVERAGE_ALL_FEATURES ?= 0

ifeq ($(strip $(COVERAGE_ALL_FEATURES)),1)
	COVERAGE_FEATURE_ARGS := --all-features
else ifneq ($(strip $(FEATURES)),)
	COVERAGE_FEATURE_ARGS := --features "$(FEATURES)"
else
	COVERAGE_FEATURE_ARGS :=
endif

# Rust configuration / Rust配置
CARGO := cargo
RUSTC_VERSION := $(shell rustc --version 2>/dev/null || echo "unknown")

WEB_ADMIN_DIR := web-admin
WEB_CONSOLE_DIR := web-console

CLIPPY_DENY_WARNINGS ?= 0

NOCAPTURE ?= 1

# Colors for output / 输出颜色
RED := \033[0;31m
GREEN := \033[0;32m
YELLOW := \033[1;33m
BLUE := \033[0;34m
NC := \033[0m # No Color

.PHONY: all build build-release test test-ui test-mic-device test-sled test-rocksdb test-all-features test-ui clean clean-coverage coverage coverage-quick coverage-llvm coverage-html coverage-lcov coverage-no-fail coverage-open install-deps format format-check lint check doc help bench audit outdated ci dev info e2e e2e-docker e2e-linux e2e-kind mac-build mac-build-release web-admin-build web-admin-lint web-admin-test web-console-build web-console-lint web-console-test samples image-spear
.DEFAULT_GOAL := build

# Default target / 默认目标
all: check build

# Display help information / 显示帮助信息
help:
	@echo -e "$(BLUE)SPEAR Next Build System / SPEAR Next构建系统$(NC)"
	@echo "=================================================="
	@echo ""
	@echo -e "$(GREEN)Available targets / 可用目标:$(NC)"
	@echo "  build           - Build the project / 构建项目"
	@echo "  test            - Run all tests / 运行所有测试"
	@echo "  coverage        - Run comprehensive code coverage analysis / 运行全面代码覆盖率分析"
	@echo "  coverage-quick  - Run quick code coverage analysis / 运行快速代码覆盖率分析"
	@echo "  coverage-html   - Generate HTML coverage report (llvm-cov) / 生成HTML覆盖率报告（llvm-cov）"
	@echo "  coverage-lcov   - Generate LCOV coverage report (llvm-cov) / 生成LCOV覆盖率报告（llvm-cov）"
	@echo "  clean-coverage  - Clean coverage artifacts / 清理覆盖率产物"
	@echo "  clean           - Clean build artifacts / 清理构建产物"
	@echo "  format          - Format code / 格式化代码"
	@echo "  lint            - Run linter / 运行代码检查"
	@echo "  check           - Run cargo check / 运行cargo检查"
	@echo "  doc             - Generate documentation / 生成文档"
	@echo "  image-spear      - Build unified SPEAR Docker image / 构建统一SPEAR Docker镜像"
	@echo "  install-deps    - Install development dependencies / 安装开发依赖"
	@echo "  help            - Show this help message / 显示此帮助信息"
	@echo "  e2e             - Run all E2E tests / 运行所有端到端测试"
	@echo "  e2e-docker      - Run Docker-based E2E tests / 运行基于Docker的端到端测试"
	@echo "  e2e-kind        - Run kind+Helm E2E tests / 运行基于kind+Helm的端到端测试"
	@echo "  samples         - Build WASM samples / 构建WASM示例"
	@echo "  web-admin-build - Build Web Admin assets / 构建Web Admin静态资源"
	@echo "  web-admin-test  - Run Web Admin tests / 运行Web Admin测试"
	@echo "  web-admin-lint  - Lint Web Admin / Web Admin代码检查"
	@echo "  web-console-build - Build SPEAR Console assets / 构建SPEAR Console静态资源"
	@echo ""
	@echo -e "$(YELLOW)Examples / 示例:$(NC)"
	@echo "  make build                    # Build with default features / 使用默认特性构建"
	@echo "  make test                     # Run all tests / 运行所有测试"
	@echo "  make test NOCAPTURE=0          # Hide test output / 隐藏测试输出"
	@echo "  make coverage-quick           # Quick coverage analysis / 快速覆盖率分析"
	@echo "  make FEATURES=sled build      # Build with sled feature / 使用sled特性构建"
	@echo "  make image-spear IMAGE_REPO=spear IMAGE_TAG=local # Build unified image / 构建统一镜像"
	@echo ""

# Install development dependencies / 安装开发依赖
install-deps:
	@echo -e "$(BLUE)📦 Installing development dependencies... / 安装开发依赖...$(NC)"
	@if ! command -v cargo-llvm-cov >/dev/null 2>&1; then \
		echo -e "$(YELLOW)Installing cargo-llvm-cov... / 安装cargo-llvm-cov...$(NC)"; \
		$(CARGO) install cargo-llvm-cov; \
	fi
	@if command -v rustup >/dev/null 2>&1; then \
		rustup component add llvm-tools-preview >/dev/null 2>&1 || true; \
	fi
	@if ! command -v cargo-tarpaulin >/dev/null 2>&1; then \
		echo -e "$(YELLOW)Installing cargo-tarpaulin... / 安装cargo-tarpaulin...$(NC)"; \
		$(CARGO) install cargo-tarpaulin; \
	fi
	@if ! command -v cargo-audit >/dev/null 2>&1; then \
		echo -e "$(YELLOW)Installing cargo-audit... / 安装cargo-audit...$(NC)"; \
		$(CARGO) install cargo-audit || echo -e "$(YELLOW)⚠️  cargo-audit installation failed (version compatibility issue) / cargo-audit安装失败（版本兼容性问题）$(NC)"; \
	fi
	@if ! command -v cargo-outdated >/dev/null 2>&1; then \
		echo -e "$(YELLOW)Installing cargo-outdated... / 安装cargo-outdated...$(NC)"; \
		$(CARGO) install cargo-outdated || echo -e "$(YELLOW)⚠️  cargo-outdated installation failed / cargo-outdated安装失败$(NC)"; \
	fi
	@echo -e "$(GREEN)✅ Development dependencies installation completed / 开发依赖安装完成$(NC)"

# Build the project / 构建项目
build: web-admin-build web-console-build
	@echo -e "$(BLUE)🔨 Building $(PROJECT_NAME)... / 构建$(PROJECT_NAME)...$(NC)"
	@if [ -n "$(FEATURES)" ]; then \
		echo -e "$(YELLOW)Building with features: $(FEATURES) / 使用特性构建: $(FEATURES)$(NC)"; \
		$(CARGO) build --features $(FEATURES); \
	else \
		$(CARGO) build; \
	fi
	@echo -e "$(GREEN)✅ Build completed / 构建完成$(NC)"

# Build release version / 构建发布版本
build-release: web-admin-build web-console-build
	@echo -e "$(BLUE)🚀 Building release version... / 构建发布版本...$(NC)"
	@if [ -n "$(FEATURES)" ]; then \
		$(CARGO) build --release --features $(FEATURES); \
	else \
		$(CARGO) build --release; \
	fi
	@echo -e "$(GREEN)✅ Release build completed / 发布版本构建完成$(NC)"

# Run tests / 运行测试
test:
	@echo -e "$(BLUE)🧪 Running tests... / 运行测试...$(NC)"
	@NOCAPTURE_ARGS=""; \
	if [ "$(NOCAPTURE)" = "1" ]; then \
		NOCAPTURE_ARGS="-- --nocapture"; \
	fi; \
	if [ -n "$(FEATURES)" ]; then \
		$(CARGO) test --features $(FEATURES) $$NOCAPTURE_ARGS; \
	else \
		$(CARGO) test $$NOCAPTURE_ARGS; \
	fi
	@$(MAKE) web-admin-test
	@$(MAKE) web-console-test
	@echo -e "$(GREEN)✅ Tests completed / 测试完成$(NC)"

.PHONY: web-admin-build web-admin-lint web-admin-test web-console-build web-console-lint web-console-test
web-admin-build:
	@echo -e "$(BLUE)🔧 Building Web Admin assets... / 构建Web Admin静态资源...$(NC)"
	@if ! command -v npm >/dev/null 2>&1; then \
		if [ -f "assets/admin/index.html" ] && [ -f "assets/admin/main.js" ] && [ -f "assets/admin/main.css" ]; then \
			echo -e "$(YELLOW)⚠️ npm not found, using existing assets/admin/* / 未找到npm，使用已有assets/admin/*$(NC)"; \
			exit 0; \
		else \
			echo -e "$(RED)❌ npm not found and assets/admin/* missing. Install npm or run in an environment with Node. / 未找到npm且assets/admin/*不存在，请安装Node/npm$(NC)"; \
			exit 1; \
		fi; \
	fi
	@cd $(WEB_ADMIN_DIR) && \
		(if [ -f package-lock.json ]; then npm ci --silent; else npm install --silent; fi) && \
		npm run build
	@echo -e "$(GREEN)✅ Web Admin assets built / Web Admin静态资源构建完成$(NC)"

web-console-build:
	@echo -e "$(BLUE)🔧 Building SPEAR Console assets... / 构建SPEAR Console静态资源...$(NC)"
	@if ! command -v npm >/dev/null 2>&1; then \
		if [ -f "assets/console/index.html" ] && [ -f "assets/console/main.js" ] && [ -f "assets/console/main.css" ]; then \
			echo -e "$(YELLOW)⚠️ npm not found, using existing assets/console/* / 未找到npm，使用已有assets/console/*$(NC)"; \
			exit 0; \
		else \
			echo -e "$(RED)❌ npm not found and assets/console/* missing. Install npm or run in an environment with Node. / 未找到npm且assets/console/*不存在，请安装Node/npm$(NC)"; \
			exit 1; \
		fi; \
	fi
	@cd $(WEB_CONSOLE_DIR) && \
		(if [ -f package-lock.json ]; then npm ci --silent; else npm install --silent; fi) && \
		npm run build
	@echo -e "$(GREEN)✅ SPEAR Console assets built / SPEAR Console静态资源构建完成$(NC)"

web-console-lint:
	@echo -e "$(BLUE)🔍 Linting SPEAR Console... / SPEAR Console代码检查...$(NC)"
	@if ! command -v npm >/dev/null 2>&1; then \
		echo -e "$(YELLOW)⚠️ npm not found, skipping SPEAR Console lint / 未找到npm，跳过SPEAR Console代码检查$(NC)"; \
		exit 0; \
	fi
	@cd $(WEB_CONSOLE_DIR) && \
		(if [ -f package-lock.json ]; then npm ci --silent; else npm install --silent; fi) && \
		npm run lint
	@echo -e "$(GREEN)✅ SPEAR Console lint completed / SPEAR Console代码检查完成$(NC)"

web-console-test:
	@echo -e "$(BLUE)🧪 Running SPEAR Console tests... / 运行SPEAR Console测试...$(NC)"
	@if ! command -v npm >/dev/null 2>&1; then \
		echo -e "$(YELLOW)⚠️ npm not found, skipping SPEAR Console tests / 未找到npm，跳过SPEAR Console测试$(NC)"; \
		exit 0; \
	fi
	@cd $(WEB_CONSOLE_DIR) && \
		(if [ -f package-lock.json ]; then npm ci --silent; else npm install --silent; fi) && \
		npm test
	@echo -e "$(GREEN)✅ SPEAR Console tests completed / SPEAR Console测试完成$(NC)"

web-admin-lint:
	@echo -e "$(BLUE)🔍 Linting Web Admin... / Web Admin代码检查...$(NC)"
	@if ! command -v npm >/dev/null 2>&1; then \
		echo -e "$(YELLOW)⚠️ npm not found, skipping Web Admin lint / 未找到npm，跳过Web Admin代码检查$(NC)"; \
		exit 0; \
	fi
	@cd $(WEB_ADMIN_DIR) && \
		(if [ -f package-lock.json ]; then npm ci --silent; else npm install --silent; fi) && \
		npm run lint
	@echo -e "$(GREEN)✅ Web Admin lint completed / Web Admin代码检查完成$(NC)"

web-admin-test:
	@echo -e "$(BLUE)🧪 Running Web Admin tests... / 运行Web Admin测试...$(NC)"
	@if ! command -v npm >/dev/null 2>&1; then \
		echo -e "$(YELLOW)⚠️ npm not found, skipping Web Admin tests / 未找到npm，跳过Web Admin测试$(NC)"; \
		exit 0; \
	fi
	@cd $(WEB_ADMIN_DIR) && \
		(if [ -f package-lock.json ]; then npm ci --silent; else npm install --silent; fi) && \
		npm test
	@echo -e "$(GREEN)✅ Web Admin tests completed / Web Admin测试完成$(NC)"

test-mic-device:
	@echo -e "$(BLUE)🧪 Running mic-device capture test... / 运行mic-device采集测试...$(NC)"
	$(CARGO) test --features mic-device test_mic_device_returns_pcm16_frames -- --nocapture --test-threads=1

mac-build:
	@$(MAKE) build FEATURES="$(FEATURES) mic-device"

mac-build-release:
	@$(MAKE) build-release FEATURES="$(FEATURES) mic-device"


.PHONY: test-ui
test-ui:
	@echo -e "$(BLUE)🧪 Running UI tests... / 运行UI测试...$(NC)"
	@if ! command -v npm >/dev/null 2>&1; then \
		echo -e "$(YELLOW)⚠️ npm not found, skipping UI tests / 未找到npm，跳过UI测试$(NC)"; \
		exit 0; \
	fi
	@{ \
		PID=""; \
		if command -v pgrep >/dev/null 2>&1; then \
			PID=$$(pgrep -f "target/.*/sms .*--web-admin-addr 127.0.0.1:8081" || true); \
		fi; \
		if [ -n "$$PID" ]; then \
			echo -e "$(YELLOW)⚠️ stopping existing sms web-admin server (pid=$$PID) / 停止已有sms web-admin进程$(NC)"; \
			kill $$PID >/dev/null 2>&1 || true; \
			sleep 1; \
		fi; \
		if command -v lsof >/dev/null 2>&1; then \
			PID=$$(lsof -ti tcp:8081 2>/dev/null || true); \
			if [ -n "$$PID" ]; then \
				echo -e "$(YELLOW)⚠️ stopping process on :8081 (pid=$$PID) / 停止占用8081端口进程$(NC)"; \
				kill $$PID >/dev/null 2>&1 || true; \
				sleep 1; \
			fi; \
			PID=$$(lsof -ti tcp:8080 2>/dev/null || true); \
			if [ -n "$$PID" ]; then \
				echo -e "$(YELLOW)⚠️ stopping process on :8080 (pid=$$PID) / 停止占用8080端口进程$(NC)"; \
				kill $$PID >/dev/null 2>&1 || true; \
				sleep 1; \
			fi; \
		fi; \
	}
	@$(MAKE) web-admin-build
	@cd ui-tests && \
		npm install --silent && \
		npm run install:pw --silent || true && \
		npm test
	@echo -e "$(GREEN)✅ UI tests completed / UI测试完成$(NC)"

# Run tests with specific feature / 运行特定特性的测试
test-sled:
	@echo -e "$(BLUE)🧪 Running tests with sled feature... / 运行sled特性测试...$(NC)"
	$(CARGO) test --features sled

test-rocksdb:
	@echo -e "$(BLUE)🧪 Running tests with rocksdb feature... / 运行rocksdb特性测试...$(NC)"
	$(CARGO) test --features rocksdb

test-all-features:
	@echo -e "$(BLUE)🧪 Running tests with all features... / 运行所有特性测试...$(NC)"
	$(CARGO) test --all-features

# Run comprehensive code coverage analysis / 运行全面代码覆盖率分析
coverage: install-deps
	@echo -e "$(BLUE)📊 Running comprehensive code coverage analysis... / 运行全面代码覆盖率分析...$(NC)"
	@if command -v cargo-llvm-cov >/dev/null 2>&1; then \
		$(MAKE) coverage-llvm COVERAGE_ALL_FEATURES="$(COVERAGE_ALL_FEATURES)" FEATURES="$(FEATURES)"; \
	else \
		./scripts/coverage.sh; \
	fi
	@echo -e "$(GREEN)✅ Coverage analysis completed / 覆盖率分析完成$(NC)"

# Run quick code coverage analysis / 运行快速代码覆盖率分析
coverage-quick:
	@echo -e "$(BLUE)📊 Running quick coverage analysis... / 运行快速覆盖率分析...$(NC)"
	@./scripts/quick-coverage.sh
	@echo -e "$(GREEN)✅ Quick coverage analysis completed / 快速覆盖率分析完成$(NC)"

coverage-llvm: coverage-html

coverage-html: install-deps clean-coverage
	@echo -e "$(BLUE)📊 Running coverage (llvm-cov html)... / 运行覆盖率（llvm-cov html）...$(NC)"
	@mkdir -p "$(COVERAGE_DIR)"
	@$(CARGO) llvm-cov --workspace $(COVERAGE_FEATURE_ARGS) --html --output-dir "$(COVERAGE_DIR)"
	@echo -e "$(GREEN)✅ Coverage report generated: $(COVERAGE_DIR)/index.html$(NC)"

coverage-lcov: install-deps clean-coverage
	@echo -e "$(BLUE)📊 Running coverage (llvm-cov lcov)... / 运行覆盖率（llvm-cov lcov）...$(NC)"
	@mkdir -p "$(COVERAGE_DIR)"
	@$(CARGO) llvm-cov --workspace $(COVERAGE_FEATURE_ARGS) --lcov --output-path "$(COVERAGE_DIR)/lcov.info"
	@echo -e "$(GREEN)✅ Coverage report generated: $(COVERAGE_DIR)/lcov.info$(NC)"

coverage-all-features:
	@$(MAKE) coverage COVERAGE_ALL_FEATURES=1

# Run coverage analysis without failure threshold / 运行覆盖率分析但不检查失败阈值
coverage-no-fail:
	@echo -e "$(BLUE)📊 Running coverage analysis (no failure threshold)... / 运行覆盖率分析（无失败阈值）...$(NC)"
	@if ! command -v cargo-tarpaulin >/dev/null 2>&1; then \
		echo -e "$(YELLOW)Installing cargo-tarpaulin... / 安装cargo-tarpaulin...$(NC)"; \
		$(CARGO) install cargo-tarpaulin; \
	fi
	@mkdir -p target/coverage
	@$(CARGO) tarpaulin --config tarpaulin.toml --output-dir target/coverage --out Html --out Lcov --out Json --timeout 120 --verbose --ignore-panics || true
	@echo -e "$(GREEN)✅ Coverage analysis completed (check target/coverage/tarpaulin-report.html) / 覆盖率分析完成（查看target/coverage/tarpaulin-report.html）$(NC)"

# Open coverage report / 打开覆盖率报告
coverage-open:
	@if [ -f "$(COVERAGE_DIR)/index.html" ]; then \
		echo -e "$(BLUE)🌐 Opening comprehensive coverage report... / 打开全面覆盖率报告...$(NC)"; \
		open "$(COVERAGE_DIR)/index.html" 2>/dev/null || xdg-open "$(COVERAGE_DIR)/index.html" 2>/dev/null || echo -e "$(YELLOW)Please open $(COVERAGE_DIR)/index.html manually / 请手动打开$(COVERAGE_DIR)/index.html$(NC)"; \
	elif [ -f "$(COVERAGE_DIR)/tarpaulin-report.html" ]; then \
		echo -e "$(BLUE)🌐 Opening quick coverage report... / 打开快速覆盖率报告...$(NC)"; \
		open "$(COVERAGE_DIR)/tarpaulin-report.html" 2>/dev/null || xdg-open "$(COVERAGE_DIR)/tarpaulin-report.html" 2>/dev/null || echo -e "$(YELLOW)Please open $(COVERAGE_DIR)/tarpaulin-report.html manually / 请手动打开$(COVERAGE_DIR)/tarpaulin-report.html$(NC)"; \
	else \
		echo -e "$(RED)❌ No coverage report found. Run 'make coverage' or 'make coverage-quick' first / 未找到覆盖率报告。请先运行'make coverage'或'make coverage-quick'$(NC)"; \
	fi

clean-coverage:
	rm -rf "$(COVERAGE_DIR)"

# Clean build artifacts / 清理构建产物
clean:
	@echo -e "$(BLUE)🧹 Cleaning build artifacts... / 清理构建产物...$(NC)"
	$(CARGO) clean
	rm -rf $(COVERAGE_DIR)
	@echo -e "$(GREEN)✅ Clean completed / 清理完成$(NC)"

# Format code / 格式化代码
format:
	@echo -e "$(BLUE)🎨 Formatting code... / 格式化代码...$(NC)"
	$(CARGO) fmt
	@echo -e "$(GREEN)✅ Code formatted / 代码格式化完成$(NC)"

# Check code formatting / 检查代码格式
format-check:
	@echo -e "$(BLUE)🔍 Checking code formatting... / 检查代码格式...$(NC)"
	$(CARGO) fmt --check

# Run linter / 运行代码检查
lint:
	@echo -e "$(BLUE)🔍 Running linter... / 运行代码检查...$(NC)"
	@if [ "$(CLIPPY_DENY_WARNINGS)" = "1" ]; then \
		$(CARGO) clippy --all-targets -- -D warnings; \
	else \
		$(CARGO) clippy --all-targets; \
	fi
	@$(MAKE) web-admin-lint
	@$(MAKE) web-console-lint
	@echo -e "$(GREEN)✅ Linting completed / 代码检查完成$(NC)"

# Run cargo check / 运行cargo检查
check:
	@echo -e "$(BLUE)✅ Running cargo check... / 运行cargo检查...$(NC)"
	$(CARGO) check
	@if [ -n "$(FEATURES)" ]; then \
		$(CARGO) check --features $(FEATURES); \
	fi
	@echo -e "$(GREEN)✅ Check completed / 检查完成$(NC)"

# Generate documentation / 生成文档
doc:
	@echo -e "$(BLUE)📚 Generating documentation... / 生成文档...$(NC)"
	$(CARGO) doc --no-deps --open
	@echo -e "$(GREEN)✅ Documentation generated / 文档生成完成$(NC)"

# Run benchmarks / 运行基准测试
bench:
	@echo -e "$(BLUE)⚡ Running benchmarks... / 运行基准测试...$(NC)"
	$(CARGO) bench
	@echo -e "$(GREEN)✅ Benchmarks completed / 基准测试完成$(NC)"

# Build WASM samples
.PHONY: samples
samples:
	@echo -e "$(BLUE)🔨 Building WASM samples... / 构建WASM示例...$(NC)"
	@mkdir -p $(SAMPLES_BUILD)
	@if command -v zig >/dev/null 2>&1; then \
		for name in hello chat_completion chat_completion_tool_sum mic_rtasr mcp_fs user_stream_echo; do \
			src="$(SAMPLES_DIR)/$$name.c"; \
			out="$(SAMPLES_BUILD)/$$name.wasm"; \
			extra_ld=""; \
			case "$$name" in \
				chat_completion|chat_completion_tool_sum|mcp_fs) extra_ld="-Wl,--export-memory" ;; \
			esac; \
			zig cc -target wasm32-wasi -O2 -Isdk/c/include $(SAMPLES_CFLAGS) -Wl,--export-table $$extra_ld -o "$$out" "$$src" || (echo -e "$(RED)❌ zig wasm32-wasi build failed. Install wasi-sdk or zig$(NC)"; exit 1); \
			[ -f "$$out" ] && echo -e "$(GREEN)✅ Built with zig: $$out$(NC)" || (echo -e "$(RED)❌ zig output missing. Install zig or set WASI_SYSROOT$(NC)"; exit 1); \
		done; \
	else \
		if command -v clang >/dev/null 2>&1 && [ -n "$(WASI_SYSROOT)" ]; then \
			for name in hello chat_completion chat_completion_tool_sum mic_rtasr mcp_fs user_stream_echo; do \
				src="$(SAMPLES_DIR)/$$name.c"; \
				out="$(SAMPLES_BUILD)/$$name.wasm"; \
				extra_ld=""; \
				case "$$name" in \
					chat_completion|chat_completion_tool_sum|mcp_fs) extra_ld="-Wl,--export-memory" ;; \
				esac; \
				clang --target=wasm32-wasi --sysroot=$(WASI_SYSROOT) -O2 -Isdk/c/include $(SAMPLES_CFLAGS) -Wl,--export-table $$extra_ld -o "$$out" "$$src" || (echo -e "$(RED)❌ clang wasm32-wasi build failed. Install wasi-sdk or zig$(NC)"; exit 1); \
				[ -f "$$out" ] && echo -e "$(GREEN)✅ Built with clang: $$out$(NC)" || (echo -e "$(RED)❌ clang output missing. Install zig or set WASI_SYSROOT$(NC)"; exit 1); \
				done; \
		else \
			echo -e "$(RED)❌ No suitable compiler found (zig, or clang+WASI_SYSROOT). Install zig or set WASI_SYSROOT$(NC)"; exit 1; \
		fi; \
	fi
	@if [ "$(BUILD_JS_SAMPLES)" = "1" ]; then \
		if command -v cargo >/dev/null 2>&1; then \
			echo -e "$(BLUE)🟨 Building WASM-JS samples... / 构建WASM-JS示例...$(NC)"; \
			rm -rf "$(SAMPLES_BUILD)/rust"; \
			mkdir -p "$(SAMPLES_JS_BUILD)"; \
			for name in $(JS_SAMPLES); do \
				dir="$(REPO_ROOT)/$(SAMPLES_JS_DIR)/$$name"; \
				if [ ! -f "$$dir/Cargo.toml" ]; then \
					echo -e "$(YELLOW)⚠️  Sample missing Cargo.toml: $$dir (skip) / 缺少Cargo.toml，跳过$(NC)"; \
					continue; \
				fi; \
				( cd "$$dir" && cargo build --release --target wasm32-wasip1 ) || (echo -e "$(RED)❌ wasm-js build failed: $$name (need rustup target wasm32-wasip1) / WASM-JS构建失败（需要安装wasm32-wasip1目标）$(NC)"; exit 1); \
				in="$$dir/target/wasm32-wasip1/release/$$name.wasm"; \
				out_js="$(SAMPLES_JS_BUILD)/$(JS_WASM_PREFIX)$$name.wasm"; \
				if [ -f "$$in" ]; then \
					cp "$$in" "$$out_js"; \
					echo -e "$(GREEN)✅ Built WASM-JS sample: $$out_js$(NC)"; \
				else \
					echo -e "$(RED)❌ WASM-JS output missing: $$in$(NC)"; exit 1; \
				fi; \
			done; \
		else \
			echo -e "$(YELLOW)⚠️  cargo not found, skipping WASM-JS samples / 未找到cargo，跳过WASM-JS示例$(NC)"; \
		fi; \
	fi
	@echo -e "$(GREEN)✅ Samples build completed / 示例构建完成$(NC)"


# Docker image options / Docker镜像选项
IMAGE_REPO ?= spear
IMAGE_TAG ?= local
SPEAR_IMAGE_TARGET ?=
USE_CARGO_MIRROR ?= 1
CARGO_MIRROR_REGISTRY ?= sparse+https://rsproxy.cn/index/
DEBIAN_SUITE ?= trixie

# Build unified SPEAR image (sms + spearlet) / 构建统一SPEAR镜像（sms + spearlet）
image-spear:
	@echo -e "$(BLUE)🐳 Building unified SPEAR image... / 构建统一SPEAR镜像...$(NC)"
	@if ! command -v docker >/dev/null 2>&1; then \
		echo -e "$(RED)❌ docker not found. Install Docker first. / 未找到docker，请先安装Docker$(NC)"; \
		exit 1; \
	fi
	@TARGET_ARGS=""; \
	if [ -n "$(SPEAR_IMAGE_TARGET)" ]; then \
		TARGET_ARGS="--target $(SPEAR_IMAGE_TARGET)"; \
	fi; \
	docker build $$TARGET_ARGS \
		-f deploy/docker/spear/Dockerfile \
		--build-arg "DEBIAN_SUITE=$(DEBIAN_SUITE)" \
		--build-arg "USE_CARGO_MIRROR=$(USE_CARGO_MIRROR)" \
		--build-arg "CARGO_MIRROR_REGISTRY=$(CARGO_MIRROR_REGISTRY)" \
		-t "$(IMAGE_REPO):$(IMAGE_TAG)" \
		.
	@echo -e "$(GREEN)✅ Unified SPEAR image built: $(IMAGE_REPO):$(IMAGE_TAG) / 统一SPEAR镜像构建完成$(NC)"


# Security audit / 安全审计
audit: install-deps
	@echo -e "$(BLUE)🔒 Running security audit... / 运行安全审计...$(NC)"
	@if command -v cargo-audit >/dev/null 2>&1; then \
		$(CARGO) audit; \
		echo -e "$(GREEN)✅ Security audit completed / 安全审计完成$(NC)"; \
	else \
		echo -e "$(YELLOW)⚠️  cargo-audit not available, skipping security audit / cargo-audit不可用，跳过安全审计$(NC)"; \
	fi

# Check for outdated dependencies / 检查过时的依赖
outdated: install-deps
	@echo -e "$(BLUE)📦 Checking for outdated dependencies... / 检查过时的依赖...$(NC)"
	@if command -v cargo-outdated >/dev/null 2>&1; then \
		$(CARGO) outdated; \
		echo -e "$(GREEN)✅ Dependency check completed / 依赖检查完成$(NC)"; \
	else \
		echo -e "$(YELLOW)⚠️  cargo-outdated not available, skipping dependency check / cargo-outdated不可用，跳过依赖检查$(NC)"; \
	fi

# Full CI pipeline / 完整CI流水线
ci: format-check
	@$(MAKE) lint CLIPPY_DENY_WARNINGS=1
	@$(MAKE) check
	@$(MAKE) test
	@$(MAKE) coverage-quick
	@echo -e "$(GREEN)🎉 CI pipeline completed successfully! / CI流水线成功完成！$(NC)"

# Development workflow / 开发工作流
dev: format lint test
	@echo -e "$(GREEN)🚀 Development workflow completed! / 开发工作流完成！$(NC)"

# Show project information / 显示项目信息
info:
	@echo -e "$(BLUE)📋 Project Information / 项目信息$(NC)"
	@echo "=================================================="
	@echo "Project Name / 项目名称: $(PROJECT_NAME)"
	@echo "Version / 版本: $(VERSION)"
	@echo "Rust Version / Rust版本: $(RUSTC_VERSION)"
	@echo "Repository Root / 仓库根目录: $(REPO_ROOT)"
	@echo "Target Directory / 目标目录: $(TARGET_DIR)"
	@echo "Coverage Directory / 覆盖率目录: $(COVERAGE_DIR)"
	@echo ""

# Run all E2E tests / 运行所有端到端测试
e2e:
	@echo -e "$(BLUE)🧪 Running E2E tests... / 运行端到端测试...$(NC)"
	@bash scripts/e2e.sh
	@echo -e "$(GREEN)✅ E2E tests completed / 端到端测试完成$(NC)"

.PHONY: e2e-docker
e2e-docker:
	@echo -e "$(BLUE)🧪 Running E2E tests with Docker... / 使用Docker运行端到端测试...$(NC)"
	@bash scripts/e2e-docker.sh
	@echo -e "$(GREEN)✅ E2E tests (Docker) completed / 端到端测试（Docker）完成$(NC)"

.PHONY: e2e-kind
e2e-kind:
	@echo -e "$(BLUE)🧪 Running E2E tests with kind+Helm... / 使用kind+Helm运行端到端测试...$(NC)"
	@bash scripts/e2e-kind.sh
	@echo -e "$(GREEN)✅ E2E tests (kind) completed / 端到端测试（kind）完成$(NC)"

.PHONY: e2e-linux
e2e-linux:
	@echo -e "$(BLUE)🧪 Running E2E tests with Linux binaries... / 使用Linux二进制运行端到端测试...$(NC)"
	@if command -v cross >/dev/null 2>&1; then \
		echo -e "$(BLUE)📦 cross detected, building Linux GNU target... / 检测到cross，正在构建Linux GNU目标...$(NC)"; \
		SKIP_PROTOC=1 CROSS_CONTAINER_ENV_SKIP_PROTOC=1 DOCKER_DEFAULT_PLATFORM=linux/amd64 cross build --release --target x86_64-unknown-linux-gnu --bins || (echo -e "$(RED)❌ cross build failed / cross构建失败$(NC)"; exit 1); \
		DOCKER_DEFAULT_PLATFORM=linux/amd64 E2E_BIN_DIR=$(TARGET_DIR)/x86_64-unknown-linux-gnu/release DOCKER=1 $(CARGO) test --test testcontainers_e2e -- --ignored --nocapture || (echo -e "$(RED)❌ E2E tests failed / 端到端测试失败$(NC)"; exit 1); \
	else \
		echo -e "$(YELLOW)⚠️  'cross' not installed, attempting musl cross-compile... / 未安装cross，尝试musl交叉编译...$(NC)"; \
		rustup target add x86_64-unknown-linux-musl >/dev/null 2>&1 || true; \
		$(CARGO) build --release --target x86_64-unknown-linux-musl || (echo -e "$(YELLOW)⚠️  Cross compile failed, ensure musl toolchain is installed / 交叉编译失败，请确保安装musl工具链$(NC)"; echo -e "$(YELLOW)👉 Tip: install 'cross' with 'cargo install cross' / 建议使用'cargo install cross'安装cross$(NC)"; exit 1); \
		DOCKER_DEFAULT_PLATFORM=linux/amd64 E2E_BIN_DIR=$(TARGET_DIR)/x86_64-unknown-linux-musl/release DOCKER=1 $(CARGO) test --test testcontainers_e2e -- --ignored --nocapture || (echo -e "$(RED)❌ E2E tests failed / 端到端测试失败$(NC)"; exit 1); \
	fi
	@echo -e "$(GREEN)✅ E2E tests (Linux) completed / 端到端测试（Linux）完成$(NC)"
SAMPLES_DIR := samples/wasm-c
SAMPLES_BUILD := samples/build
SAMPLES_CFLAGS ?=
SAMPLES_JS_DIR ?= samples/wasm-js
SAMPLES_JS_BUILD ?= $(SAMPLES_BUILD)/js
JS_WASM_PREFIX ?= js-
JS_SAMPLES ?= chat_completion chat_completion_tool_sum router_filter_keyword user_stream_echo
BUILD_JS_SAMPLES ?= 1
SAMPLES_RUST_DIR ?= $(SAMPLES_JS_DIR)
RUST_SAMPLES ?= $(JS_SAMPLES)
BUILD_RUST_SAMPLES ?= $(BUILD_JS_SAMPLES)

ifeq ($(origin JS_SAMPLES), file)
ifeq ($(origin RUST_SAMPLES), command line)
JS_SAMPLES := $(RUST_SAMPLES)
endif
endif

ifeq ($(origin BUILD_JS_SAMPLES), file)
ifeq ($(origin BUILD_RUST_SAMPLES), command line)
BUILD_JS_SAMPLES := $(BUILD_RUST_SAMPLES)
endif
endif
