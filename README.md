# Odoo ERP MCP Server (Rust)

A high-performance, secure Model Context Protocol (MCP) server built in Rust for seamless interaction with Odoo ERP systems. This server allows AI models (like Claude, GPT-4, etc.) to perform advanced ORM operations and data analysis on Odoo models natively.

## 🚀 Features

- **Standard CRUD**: Search, Create, Update, and Delete any Odoo model.
- **Advanced Tools**: 
    - `search_count`: Get record counts matching specific criteria.
    - `read_group`: Perform aggregated analysis (group by, sums, counts).
    - `copy`: Quickly duplicate existing records.
    - `get_metadata`: Inspect model structures and field definitions.
- **Web Configuration UI**: Modern, dark/light mode interface at `http://localhost:3333` to manage multiple Odoo instances and custom AI prompts.
- **Protocol Version 2025-11-25**: Built on the latest stable MCP specification for reliability and rich metadata support.

## 🛠 Tech Stack

- **Rust**: Language of choice for performance and safety.
- **Tokio**: Asynchronous runtime for non-blocking I/O.
- **Axum**: Lightweight web framework for the configuration UI.
- **Reqwest**: For handling Odoo JSON-RPC communication.
- **Serde**: For high-speed JSON serialization.

## 📦 Getting Started

### Prerequisites

- [Rust & Cargo](https://rustup.rs/) (Stable)
- An Odoo Instance (v12 to v17 supported)

### Local Development

1. **Clone the repository**:
   ```bash
   git clone <repo-url>
   cd odoo-erp-mcp
   ```

2. **Run the server**:
   ```bash
   cargo run
   ```

3. **Configure via UI**:
   Open `http://localhost:3333` in your browser to add your Odoo instance credentials. These are saved securely in `config.json`.

### MCP Communication Flow

```mermaid
sequenceDiagram
    participant AI as AI Model (e.g. Claude)
    participant Client as MCP Client (Antigravity/Cursor)
    participant Server as Odoo MCP Server (Rust)
    participant Odoo as Odoo ERP Instance

    AI->>Client: Send "Show me the top 5 customers"
    Client->>Server: tools/call (odoo-search-read)
    Server->>Odoo: JSON-RPC (search_read)
    Odoo-->>Server: JSON Result
    Server-->>Client: Tool Result (JSON)
    Client-->>AI: Result Context
    AI->>AI: Generate Human Response
```

## 🔌 Editor Integration

You can integrate this MCP server with various AI-powered code editors across different platforms (Windows, macOS, Linux). Ensure you have compiled the server or downloaded the executable.

> **Note on Paths**: On Windows, ensure your path points to the `.exe` file using double backslashes (e.g., `C:\\path\\to\\odoo-erp-mcp.exe`) or forward slashes. On Linux/macOS, point to the generated binary directly (e.g., `/path/to/odoo-erp-mcp`).

### Cursor
1. Go to **Cursor Settings** > **Features** > **MCP Servers**.
2. Click **+ Add new MCP server**.
3. Set the **Name** to `odoo-mcp`.
4. Set the **Type** to `command`.
5. Set the **Command** to the absolute path of your `odoo-erp-mcp` executable.

### Antigravity
Add the server to your `mcp_config.json` (typically located in `~/.gemini/antigravity/mcp_config.json` or `%USERPROFILE%\.gemini\antigravity\mcp_config.json`):

```json
{
  "mcpServers": {
    "odoo-mcp": {
      "command": "absolute/path/to/odoo-erp-mcp",
      "args": [],
      "env": {}
    }
  }
}
```

### Visual Studio Code (via Extensions)
If you use MCP clients in VSCode (e.g., *Cline* or *Claude Dev*), edit your MCP settings file (e.g., `cline_mcp_settings.json`):

```json
{
  "mcpServers": {
    "odoo-mcp": {
      "command": "absolute/path/to/odoo-erp-mcp",
      "args": [],
      "env": {}
    }
  }
}
```

### PyCharm / JetBrains IDEs
For JetBrains IDEs using an MCP or AI assistant plugin that supports standard MCP servers:
1. Open the plugin's MCP server configuration.
2. Add a new **Stdio** based server.
3. Configure the command path to the `odoo-erp-mcp` executable.

## 🚢 Deployment & Release

### Build for Release
To generate a highly optimized binary:
```bash
cargo build --release
```
The binary will be located at `./target/release/odoo-erp-mcp`.

### Deploying to Production
1. Copy the compiled binary and the `src/index.html` (if not embedded) to your server.
2. Ensure `config.json` is present or initialized via the UI on first run.
3. Add the binary path to your MCP client configuration (e.g. `claude_desktop_config.json`).

## 📄 License
This project is licensed under the GNU AGPL-3.
