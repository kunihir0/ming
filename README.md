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
4. **`crates/minibot`:** A lightweight, high-performance in-game bot built for quick team chat interactions, advanced map marker queries (like vending machine searches), and returning rich Discord embeds/native Rust icons.
5. **`crates/api`:** The backend HTTP and WebSocket API server. Exposes the bot's capabilities and map data to the frontend dashboard.
6. **`web`:** The interactive frontend dashboard built with Vue 3 and Vite, providing a rich UI for the tactical map, team chat, and remote camera feeds.

## Quick Start

1. Ensure you have Rust and Cargo installed.
2. Copy `.env.example` to `.env` and insert your Discord Bot Token:
   ```bash
   cp .env.example .env
   # Edit .env to add your DISCORD_TOKEN
   ```
3. Start all services (Bot, API, and Web frontend):
   ```bash
   node scripts/start.js
   ```
   *(Note: The bot will automatically run Diesel database migrations and create `db.sqlite` on its first boot).*
4. Invite the bot to your Discord server ensuring it has the necessary permissions (e.g., Administrator for channel creation).
5. Type `/setup` in your server to configure the channel auto-creation preferences.
6. Type `/credentials add` and fill out the secure modal with your GCM/FCM credentials.
7. Pair a server in the Rust+ app on your phone. The bot will instantly intercept the pairing and automatically generate your interactive server dashboard!

---

## Minibot Features

If you are running the new standalone Minibot (`cargo run -p minibot`):

- **Vending Machine Search**: Find items globally with `@v search <item>` in team chat. Includes smart grouping, pagination, and native Rust UI icons in-game.
- **Discord Integration**: Rich embeds with dynamic item thumbnails (via CarbonMod CDN) when searching from Discord (`/v search`).
- **Team Chat Bridging**: Seamlessly relays messages and commands directly to and from your Rust team chat.
- **FCM Interception**: Instantly detects and pairs with Rust servers via Google's Push notification servers.