# 🚀 Vibe Social API Gateway

[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org)
[![Axum](https://img.shields.io/badge/axum-0.8-blue.svg)](https://github.com/tokio-rs/axum)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A high-performance, asynchronous API Gateway built with **Rust** and **Axum**. Designed for the Vibe Social ecosystem to provide robust routing, rate limiting, and seamless WebSocket support.

---

## ✨ Key Features

-   **High Performance**: Leverages the power of Rust and the Tokio runtime for extremely low latency and high throughput.
-   **Dynamic Routing**: Sophisticated path matching with support for exact matches, prefix matches, and root fallback.
-   **WebSocket Support**: Transparent full-duplex proxying for real-time applications (Chat, Notifications).
-   **Rate Limiting**: Built-in protection using the `governor` algorithm to prevent abuse and ensure stability.
-   **CORS Management**: Fully configurable Cross-Origin Resource Sharing.
-   **Observability**: Integrated structured logging and tracing for easy debugging and monitoring.
-   **Transparent Forwarding**: RFC-compliant header handling (`X-Forwarded-For`, `X-Forwarded-Proto`, etc.).

---

## 🛠 Tech Stack

-   **Language**: [Rust 2024 Edition](https://www.rust-lang.org/)
-   **Web Framework**: [Axum](https://github.com/tokio-rs/axum)
-   **Runtime**: [Tokio](https://tokio.rs/)
-   **HTTP Client**: [Reqwest](https://github.com/seanmonstar/reqwest)
-   **Rate Limiting**: [Tower Governor](https://github.com/m-reynolds/tower-governor)
-   **Serialization/Configuration**: [Serde](https://serde.rs/) & [Serde YAML](https://github.com/dtolnay/serde-yaml)

---

## 🚀 Getting Started

### Prerequisites

-   [Rust toolchain](https://rustup.rs/) (latest stable recommended)
-   `cargo` package manager

### Installation

1.  Clone the repository:
    ```bash
    git clone https://github.com/your-org/vibe-social-api-gateway.git
    cd api-gateway
    ```

2.  Build the project:
    ```bash
    cargo build --release
    ```

### Configuration

The gateway is configured via a `config.yml` file in the root directory.

```yaml
port: 8000
rate_limit_per_second: 50
routes:
  user-service:
    path: /api/users
    target: http://localhost:3001
    strip_prefix: false
  chat-service-ws:
    path: /ws/chat
    target: ws://localhost:3002
    strip_prefix: false
```

### Running

Start the gateway:
```bash
cargo run
```
The server will start on `0.0.0.0:8000` (or the port specified in your config).

---

## ⚙️ Configuration Guide

| Field | Description | Default |
| :--- | :--- | :--- |
| `port` | The port the gateway listens on. | `8000` |
| `rate_limit_per_second` | Global rate limit (requests per second). | `100` |
| `routes` | Map of service names to routing rules. | - |
| `routes.<name>.path` | The incoming URL path to match. | - |
| `routes.<name>.target` | The target backend URL (http, https, ws). | - |
| `routes.<name>.strip_prefix` | Whether to remove the base path before forwarding. | `false` |

---

## 🧪 Development & Testing

Run the test suite:
```bash
cargo test
```

### Project Structure
- `src/main.rs`: Entry point.
- `src/lib.rs`: Server initialization and middleware setup.
- `src/proxy.rs`: Core proxy logic and header handling.
- `src/config.rs`: Configuration parsing logic.

---

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

Built with ❤️ for Vibe Social.
