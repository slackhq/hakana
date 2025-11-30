# Hakana Client-Server Architecture

## Overview

Hakana supports a client-server architecture where:
- A **server** process maintains warm codebase state and watches for file changes via watchman
- The **CLI** can connect to request analysis results (or run standalone)
- The **LSP** can connect to use the server for analysis (or run local analysis)

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Hakana Server                          │
│  (hakana server)                                            │
│                                                             │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  Warm State                                            │ │
│  │  - SuccessfulScanData (codebase, interner, file_system)│ │
│  │  - AnalysisResult (cached issues, references)          │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                             │
│  ┌──────────────────┐   ┌───────────────────────────────┐   │
│  │  Watchman        │   │  Request Handler              │   │
│  │  File Watcher    │   │  Unix socket                  │   │
│  │                  │   │  /tmp/hakana-{hash}.sock      │   │
│  └──────────────────┘   └───────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
              ▲                           ▲
              │                           │
     ┌────────┴────────┐         ┌────────┴────────┐
     │   CLI Client    │         │   LSP Client    │
     │ hakana analyze  │         │ hakana-language-│
     │ --with-server   │         │ server          │
     └─────────────────┘         └─────────────────┘
```

## Server Mode

The server requires **watchman** to be installed and running. It uses watchman to detect file changes and re-analyze incrementally.

### Starting the Server

```bash
# Start server (runs in foreground)
hakana server

# With custom root directory
hakana server --root /path/to/project

# With custom config
hakana server --config /path/to/hakana.json
```

### Server Subcommands

```bash
# Stop a running server
hakana server stop

# Check server status
hakana server status
```

## CLI Usage

### Default Behavior

By default, `hakana analyze` checks if a server is already running:
- If a server exists → connects and retrieves issues
- If no server → runs standalone analysis

```bash
# Uses server if available, otherwise standalone
hakana analyze
```

### Explicit Modes

```bash
# Force standalone mode (ignores any running server)
hakana analyze --standalone

# Use server mode: connect to existing server or spawn one if needed
hakana analyze --with-server
```

The `--with-server` flag will:
1. Check if a server is already running
2. If not, spawn a server in the background
3. Wait for the server to be ready
4. Connect and retrieve analysis results

## Language Server (LSP)

The language server (`hakana-language-server`) can operate in two modes:

### With Server Connection

If a hakana server is running, the LSP connects to it for analysis:
- Faster startup (no initial scan needed)
- Shared codebase state with CLI
- Server handles file watching via watchman

### Local Analysis Mode

If no server is running, the LSP performs its own analysis:
- Uses VS Code's file watcher via `workspace/didChangeWatchedFiles`
- Maintains its own codebase state
- Independent of the server

The LSP automatically detects which mode to use on startup.

## Protocol

### Socket Path

The server listens on a Unix socket at:
```
/tmp/hakana-{project_hash}.sock
```

Where `project_hash` is derived from the canonical project root path, allowing multiple servers for different projects.

### Message Format

All messages use a binary format:
```
┌──────────────┬──────────────┬─────────────────┐
│ Length (u32) │ Type (u8)    │ Payload (bytes) │
└──────────────┴──────────────┴─────────────────┘
```

### Request/Response Types

| Type | Code | Description |
|------|------|-------------|
| GetIssuesRequest | 0x06 | Get current analysis issues |
| StatusRequest | 0x10 | Get server status |
| ShutdownRequest | 0x0F | Request server shutdown |
| GotoDefinitionRequest | 0x03 | Find definition location |
| FindReferencesRequest | 0x04 | Find all references |
| FileChangedNotification | 0x05 | Notify of file changes |
| GetIssuesResponse | 0x85 | Response with issues |
| StatusResponse | 0x90 | Server status info |
| AckResponse | 0x8F | Acknowledgment |
| ErrorResponse | 0xFF | Error response |

### Connection Model

The server handles **one request per connection**. Clients connect, send a request, receive a response, and disconnect. This simplifies the server implementation and avoids connection state management.

## File Structure

```
hakana-core/src/
├── protocol/
│   ├── lib.rs          # Re-exports
│   ├── types.rs        # Request/Response structs
│   ├── serialize.rs    # Binary serialization
│   └── socket.rs       # Unix socket utilities
├── server/
│   ├── lib.rs          # Server main loop
│   ├── handler.rs      # Request handlers
│   ├── state.rs        # Warm state management
│   └── watchman.rs     # Watchman integration
└── language_server/
    ├── lib.rs          # LSP implementation
    └── server_client.rs # Client for connecting to hakana server
```

## State Management

The server maintains:
1. **SuccessfulScanData** - Codebase symbols, interner, file system state
2. **AnalysisResult** - Last analysis results for incremental updates
3. **Config** - Loaded once at startup from hakana.json

State is updated after watchman detects file changes and incremental analysis completes.

## Concurrency

- Server runs a single-threaded event loop
- One analysis at a time (analysis itself uses multiple threads internally)
- Clients receive progress information if analysis is in progress
- LSP requests can be served from cached state during analysis

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Server not running | CLI falls back to standalone, LSP uses local analysis |
| Connection timeout | Retry or fall back to standalone |
| Analysis in progress | Return progress info, client can poll |
| Watchman not available | Server fails to start with error message |
