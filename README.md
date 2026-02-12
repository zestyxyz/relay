# Relay

A federated discovery server for spatial web apps, built on [ActivityPub](https://activitypub.rocks/). Part of the Distributed Spatial Internet Graph (DSIG).

**Live instance:** [relay.zesty.xyz](https://relay.zesty.xyz)

## What is a Relay?

Relays are consensus-building servers that index and provide exposure for 3D apps. They serve as directories where users can discover spatial web experiences.

Key features:
- **App Discovery** - Browse and search indexed spatial web apps
- **Federation** - Relays can follow each other via ActivityPub, sharing their app directories
- **Beacons** - Apps verify ownership and get indexed by adding a beacon
- **Live Sessions** - Track active users across indexed apps
- **Admin Dashboard** - Manage indexed apps and federation settings
- **Customizable Frontend** - Override default templates with your own HTML

## Architecture

```
┌─────────────┐    ActivityPub    ┌─────────────┐
│   Relay A   │◄─────────────────►│   Relay B   │
└─────────────┘                   └─────────────┘
      ▲                                 ▲
      │ Beacon                          │ Beacon
      │                                 │
┌─────┴─────┐                      ┌────┴─────┐
│   App     │                      │   App    │
└───────────┘                      └──────────┘
```

When Relay A follows Relay B, all apps indexed on Relay B also appear on Relay A. Apps that appear across multiple relays are considered more "reputable" by community consensus. We have more ways of establishing and verifying reputation in the roadmap.

## Quick Start with Docker

```bash
# Clone the repo
git clone https://github.com/zestyxyz/relay.git
cd relay

# Configure environment
cp .env.sample .env
# Edit .env with your settings (make sure DB URL points to postgres:5432)

# Install sqlx CLI
cargo install sqlx-cli

# Start the server
docker compose up -d
```

## Manual Setup

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install)
- PostgreSQL
- On Ubuntu: `sudo apt install build-essential pkg-config libssl-dev postgresql`

### Installation

```bash
# Install sqlx CLI
cargo install sqlx-cli

# Configure environment
cp .env.sample .env
# Edit .env with your settings

# Create and migrate database
sqlx database create
sqlx migrate run

# Build and run
cargo run -r
```

## Configuration

| Variable | Description |
|----------|-------------|
| `DOMAIN` | Domain without protocol (e.g., `relay.example.com`) |
| `PORT` | Server port |
| `PROTOCOL` | `http://` or `https://` |
| `DATABASE_URL` | PostgreSQL connection string |
| `ADMIN_PASSWORD` | Password for `/admin` dashboard |
| `DEBUG` | Show localhost URLs (`true`/`false`) |
| `SHOW_ADULT_CONTENT` | Display adult-flagged apps (`true`/`false`) |
| `INDEX_HIDE_APPS_WITH_NO_IMAGES` | Hide apps without images on homepage |
| `GOOGLE_ANALYTICS_ID` | Optional Google Analytics tracking ID (e.g., `G-XXXXXXXXXX`) |

## Customizing the Frontend

Override default templates by creating files without the `.default` suffix:

```
frontend/
├── index.default.html    # Default homepage
├── index.html            # Your custom homepage (create this)
├── app.default.html
├── apps.default.html
├── admin.default.html
├── login.default.html
├── relays.default.html
├── error.default.html
└── styles.css
```

Templates use [Tera](https://keats.github.io/tera/) syntax.

## API Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /` | Homepage with indexed apps |
| `GET /apps` | All apps directory |
| `GET /app/:id` | Single app page |
| `GET /relays` | Federated relays list |
| `GET /relay` | ActivityPub actor |
| `POST /relay/inbox` | ActivityPub inbox |
| `GET /.well-known/webfinger` | WebFinger discovery |
| `GET /admin` | Admin dashboard |
| `POST /beacon` | Register a new beacon |

## Development

```bash
# Reset database
sqlx database drop
sqlx database create
sqlx migrate run

# Run in debug mode
DEBUG=true cargo run
```

## Documentation

- [DSIG Overview](https://docs.zesty.xyz/graph/overview)
- [Relay Documentation](https://docs.zesty.xyz/graph/relay/about)
- [Beacon Integration](https://docs.zesty.xyz/graph/beacon)

## License

MIT
