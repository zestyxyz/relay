# Relay

A federated discovery server for spatial web experiences, built on [ActivityPub](https://activitypub.rocks/). Part of the Distributed Spatial Internet Graph (DSIG).

**Live instance:** [relay.zesty.xyz](https://relay.zesty.xyz)

## What is a Relay?

Relays are consensus-building servers that index and provide exposure for 3D worlds and spatial web experiences. They serve as directories where users can discover immersive content.

Key features:
- **World Discovery** - Browse and search indexed spatial web experiences
- **SEO-Friendly URLs** - Human-readable slugs like `/world/my-awesome-world`
- **Owner Verification** - Verify ownership via meta tag to edit world details
- **Federation** - Relays can follow each other via ActivityPub, sharing their directories
- **Beacons** - Worlds get indexed by integrating a beacon script
- **Live Sessions** - Real-time tracking of active users across indexed worlds
- **Admin Dashboard** - Manage indexed worlds and federation settings
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
│   World   │                      │   World  │
└───────────┘                      └──────────┘
```

When Relay A follows Relay B, all worlds indexed on Relay B also appear on Relay A. Worlds that appear across multiple relays are considered more "reputable" by community consensus.

## Owner Verification

World owners can verify ownership and edit their world's details:

1. Visit `/world/{slug}/edit`
2. Click "Get Verification Code" to receive a unique code
3. Add the meta tag to your site's `<head>`:
   ```html
   <meta name="zesty-verify" content="your-code-here">
   ```
4. Click "Verify Ownership"
5. Once verified, you can edit: name, description, image URL, tags, adult flag

Verification uses a JWT cookie valid for 7 days.

## Quick Start with Docker

```bash
# Clone the repo
git clone https://github.com/zestyxyz/relay.git
cd relay

# Configure environment
cp .env.sample .env
# Edit .env with your settings (make sure DB URL points to postgres:5432)

# Start the server (builds locally)
docker compose -f docker-compose.local.yml up --build
```

### Docker Compose Options

| File | Command | Description |
|------|---------|-------------|
| `docker-compose.local.yml` | `docker compose -f docker-compose.local.yml up --build` | Builds from Dockerfile locally |
| `docker-compose.dev.yml` | `docker compose -f docker-compose.dev.yml up` | Uses rust image + cargo run (for development) |
| `docker-compose.yml` | `docker compose up` | Production - pulls pre-built image from ghcr.io |

**Rebuild after code changes:**
```bash
docker compose -f docker-compose.local.yml up --build --force-recreate
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
├── index.default.html    # Homepage with featured worlds
├── index.html            # Your custom homepage (create this)
├── app.default.html      # Single world detail page
├── apps.default.html     # All worlds directory
├── edit.default.html     # Owner verification & editing
├── admin.default.html    # Admin dashboard
├── login.default.html    # Admin login
├── relays.default.html   # Federated relays list
├── error.default.html    # Error page
└── styles.css
```

Templates use [Tera](https://keats.github.io/tera/) syntax.

## API Endpoints

### Public Pages
| Endpoint | Description |
|----------|-------------|
| `GET /` | Homepage with featured worlds |
| `GET /worlds` | All worlds directory |
| `GET /world/{slug}` | Single world page (also accepts numeric ID) |
| `GET /relays` | Federated relays list |

### Owner Verification & Editing
| Endpoint | Description |
|----------|-------------|
| `GET /world/{slug}/edit` | Edit page (shows verification if not verified) |
| `POST /world/{slug}/request-verification` | Get verification code |
| `POST /world/{slug}/verify` | Verify ownership via meta tag |
| `POST /world/{slug}/update` | Update world details (requires owner token) |

### Beacon & Session
| Endpoint | Description |
|----------|-------------|
| `PUT /beacon` | Register or update a world |
| `POST /session` | Send session heartbeat |
| `GET /events/sessions` | SSE stream for real-time session events |
| `GET /api/apps` | JSON API for world data |

### ActivityPub
| Endpoint | Description |
|----------|-------------|
| `GET /relay` | ActivityPub actor |
| `POST /relay/inbox` | ActivityPub inbox |
| `GET /.well-known/webfinger` | WebFinger discovery |

### Admin
| Endpoint | Description |
|----------|-------------|
| `GET /admin` | Admin dashboard (requires login) |
| `POST /admin/follow` | Follow another relay |
| `POST /admin/togglevisible` | Toggle world visibility |

## Development

### Running Locally

**Option 1: Docker (recommended)**
```bash
# Build and run with Docker
docker compose -f docker-compose.local.yml up --build

# Or use dev mode (cargo run inside container, slower but no rebuild needed)
docker compose -f docker-compose.dev.yml up
```

**Option 2: Native**
```bash
# Make sure PostgreSQL is running locally
# Run in debug mode
DEBUG=true cargo run
```

### Database Management

```bash
# Reset database
sqlx database drop
sqlx database create
sqlx migrate run
```

### URL Structure

Worlds use SEO-friendly slug URLs:
- `/world/my-awesome-world` - Slug-based URL (preferred)
- `/world/42` - Numeric ID still works for backward compatibility

Slugs are auto-generated from world names on registration. Conflicts are handled by appending numbers (`my-world`, `my-world-2`, etc.).

## Documentation

- [DSIG Overview](https://docs.zesty.xyz/graph/overview)
- [Relay Documentation](https://docs.zesty.xyz/graph/relay/about)
- [Beacon Integration](https://docs.zesty.xyz/graph/beacon)

## License

MIT
