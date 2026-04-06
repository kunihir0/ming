# Ming - Rust+ Discord Bot

Ming is a comprehensive, asynchronous Discord bot built in Rust that interfaces with the Rust+ companion app protocol. It allows server administrators and clans to manage their Rust servers directly from Discord.

## Features

- **Multi-Server Support:** Connect to and manage multiple Rust servers simultaneously from a single Discord server.
- **Auto-Provisioning Dashboards:** Automatically creates a persistent, rich Discord embed dashboard whenever you pair a new server in the Rust+ app.
- **Interactive UI:** Connect and disconnect from servers using Discord UI buttons directly on the dashboard.
- **Bidirectional Team Chat:** Intercepts and parses in-game team chat messages (integration extensible).
- **Smart Alarms & Notifications:** Designed to route in-game events like smart alarms directly to dedicated Discord channels.

## Workspace Structure

The project is structured as a Cargo workspace with three main crates to enforce a clean separation of concerns:

1. **`crates/push-receiver`:** A native Rust port of the FCM (Firebase Cloud Messaging) protocol. It intercepts Rust+ server pairing notifications directly from Google's push servers.
2. **`crates/rustplus`:** A native Rust WebSocket client for the Rust+ companion app protocol. It handles persistent server connections, real-time events, rate limiting, and camera frame rendering.
3. **`crates/bot`:** The Discord bot application. Built using `poise` and `serenity`, it manages user interactions and uses `diesel` with SQLite for relational state management.

## Quick Start

1. Ensure you have Rust and Cargo installed.
2. Copy `.env.example` to `.env` and insert your Discord Bot Token:
   ```bash
   cp .env.example .env
   # Edit .env to add your DISCORD_TOKEN
   ```
3. Run the bot:
   ```bash
   cargo run -p bot
   ```
   *(Note: The bot will automatically run Diesel database migrations and create `db.sqlite` on its first boot).*
4. Invite the bot to your Discord server ensuring it has the necessary permissions (e.g., Administrator for channel creation).
5. Type `/setup` in your server to configure the channel auto-creation preferences.
6. Type `/credentials add` and fill out the secure modal with your GCM/FCM credentials.
7. Pair a server in the Rust+ app on your phone. The bot will instantly intercept the pairing and automatically generate your interactive server dashboard!