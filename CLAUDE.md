# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Development Commands

### Building
- `cargo build --release` - Build release version
- `git submodule init && git submodule update` - Initialize HHVM submodule (required for first build)
- `cd third-party/hhvm && git apply ../../hhvm-patch.diff && cd ../../` - Apply HHVM patches for WASM compilation
- Binary is created at `./target/release/hakana-default`

### Testing
- `cargo run --release --bin=hakana test --reuse-codebase tests` - Run all tests (recommended)
- `cargo run --bin hakana test <path-to-test-dir>` - Run individual test directory
- Test directories are organized under `tests/` with subdirectories for different test types
- **IMPORTANT**: Unused variable analysis only runs for tests in `tests/unused/` directory

### Security Analysis
- `cargo run --bin hakana security-check <path>` - Run security/taint analysis mode
- When security analysis is enabled, no other analysis is performed

### Binaries
- Main binary: `hakana` (src/main.rs)
- Language server: `hakana-language-server` (src/lsp.rs)

## Architecture Overview

Hakana is a typechecker for Hack built in Rust, designed to complement HHVM's built-in typechecker with enhanced type inference and security analysis.

### Core Components

**Workspace Structure**: Multi-crate workspace with specialized modules:
- `src/analyzer/` - Core type analysis engine with expression/statement analyzers
- `src/code_info/` - Type definitions, AST structures, and codebase metadata
- `src/orchestrator/` - High-level analysis coordination and caching
- `src/cli/` - Command-line interface and test runners
- `src/language_server/` - LSP implementation
- `src/code_info_builder/` - AST scanning and initial type inference

**Analysis Architecture**:
- Expression analysis in `src/analyzer/expr/` with specialized analyzers for calls, assignments, binary operations
- Statement analysis in `src/analyzer/stmt/` covering control flow, loops, conditionals
- Reconciler system in `src/analyzer/reconciler/` for type narrowing and assertion handling
- Scope management in `src/analyzer/scope/` for tracking variable types through control flow

**Type System**:
- Union types (`t_union.rs`) and atomic types (`t_atomic.rs`) as core type representations  
- Type combination and expansion in `src/code_info/ttype/`
- Template/generic support in `src/code_info/ttype/template/`
- Type comparison logic in `src/code_info/ttype/comparison/`

**Security Analysis**:
- Taint analysis system in `src/code_info/data_flow/` 
- Tracks data flow from sources (user input) to sinks (dangerous operations)
- Uses custom attributes for annotation: `@Sink`, `@Source`, `@Sanitize`, etc.
- Security-specific analysis separate from regular type checking

### Parser Integration
- Uses HHVM's parser via git submodule in `third-party/hhvm/`
- Requires nightly Rust toolchain
- HHVM patches applied for WASM compilation support

### Plugin System
- Hook-based architecture using Rust traits (`CustomHook`, `InternalHook`)
- Hooks available for expressions, statements, arguments, definitions, parameters
- Custom builds can extend functionality via plugins

### Test Organization
- `tests/inference/` - Type inference tests organized by feature
- `tests/security/` - Taint analysis and security tests  
- `tests/diff/` - Incremental analysis tests
- `tests/fix/` - Code transformation tests
- `tests/unused/` - Unused variable and expression analysis tests
- Each test has input.hack and output.txt files

### Data Flow Analysis System
- Located in `src/analyzer/dataflow/` with core graph structures in `src/code_info/data_flow/`
- `unused_variable_analyzer.rs` contains logic for detecting unused variables and expressions
- Data flow graph tracks variable definitions, uses, and control flow boundaries
- Function analysis data (`FunctionAnalysisData`) accumulates analysis state including:
  - `data_flow_graph` - tracks variable usage patterns
  - Issue reporting and type inference state
- `report_unused_expressions()` in `functionlike_analyzer.rs` is the main entry point for unused analysis