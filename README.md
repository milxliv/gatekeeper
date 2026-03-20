# GateKeeper V2 — WBBH Visitor Management System

**Self-hosted, offline-capable visitor management for broadcast facilities.**

Zero subscriptions. Zero npm. Zero cloud dependency. One Rust binary + SQLite.

## Quick Start

```bash
# Build (requires Rust 1.75+)
cargo build --release

# Configure (copy and edit)
cp .env.example .env

# Run
./target/release/gatekeeper
# → http://localhost:3006
```

## Configuration

Environment variables (set in `.env` or shell):

| Variable | Default | Description |
|---|---|---|
| `GATEKEEPER_PASSWORD` | *(none)* | Front desk login password (required for auth) |
| `GATEKEEPER_ADMIN_PASSWORD` | *(none)* | Admin login password (falls back to front desk pw) |
| `GATEKEEPER_KIOSK_SECRET` | *(none)* | API key for kiosk check-in endpoint |
| `GATEKEEPER_DB` | `gatekeeper.db` | Path to SQLite database file |
| `GATEKEEPER_PORT` | `3000` | HTTP port |
| `GATEKEEPER_PHOTOS` | `photos` | Directory for visitor photos and logos |
| `RUST_LOG` | `info` | Log level (error, warn, info, debug, trace) |

### Microsoft Graph Integration (Optional)

For O365 calendar events and email notifications via Graph API, see `.env.example` for `GRAPH_*` variables and Azure AD setup instructions.

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│  Browser (HTMX)                                              │
│  ├── Dashboard        — live visitor status, auto-refresh    │
│  ├── Pre-Register     — staff registers expected visitors    │
│  ├── Walk-In          — receptionist logs surprise visits    │
│  ├── Group Visit      — register tour/school groups          │
│  ├── Hosts            — manage staff notification targets    │
│  ├── Visitor Log      — searchable history + date filter     │
│  ├── Admin Panel      — settings, badge branding, email      │
│  └── Badge Printing   — thermal badge generation (4"x2.4")  │
└─────────────────────────┬────────────────────────────────────┘
                          │ HTTP (no JS framework, no WebSocket)
┌─────────────────────────┴────────────────────────────────────┐
│  Axum (Rust)                                                  │
│  ├── Auth middleware    → session-based, role-aware           │
│  ├── Page routes        → full HTML responses                 │
│  ├── API routes         → HTMX partial HTML fragments         │
│  ├── Kiosk API          → JSON endpoint for tablet check-in   │
│  ├── Templates          → inline Rust, zero template engine   │
│  └── SQLite (WAL mode)  → crash-safe, offline-first           │
└──────────────────────────────────────────────────────────────┘
```

## Authentication

Two-tier role system:

- **Front desk** (`GATEKEEPER_PASSWORD`) — Dashboard, check-in/out, pre-register, walk-in, group visits, visitor log
- **Admin** (`GATEKEEPER_ADMIN_PASSWORD`) — All of the above plus settings, badge branding, host management, email config

If only `GATEKEEPER_PASSWORD` is set, it grants admin access. If neither is set, the app runs without authentication (dev mode).

Sessions use HttpOnly cookies with argon2-hashed passwords. The `Secure` flag is applied automatically when behind HTTPS (e.g., Cloudflare Tunnel).

## Visit Workflow

```
Pre-Registered Path:
  Host pre-registers visitor → Visitor arrives → Front desk confirms
  → Host notified → Check in → Photo + Badge → Check out

Walk-In Path:
  Unknown visitor arrives → Front desk enters info
  → Host gets alert (email) → Host approves/denies
  → Check in on approval → Photo + Badge → Check out

Group Visit Path:
  Staff registers group (name, size, host, purpose)
  → Group arrives → Check in group → Prints N badges → Check out
```

## Status Flow

```
pending → approved → checked_in → checked_out
    ├──→ denied
    ├──→ running_late
    └──→ rescheduled → (auto-promotes to pending when date arrives)
```

## Pages

- **/** — Dashboard with today's stats, active visits, upcoming pre-registrations
- **/pre-register** — Form for staff to register expected visitors ahead of time
- **/walk-in** — Front desk form for unannounced visitors (triggers alerts)
- **/group-visit** — Register tour groups, school visits, or large parties
- **/hosts** — Add/edit/remove staff who receive visitor notifications
- **/log** — Full searchable visitor history with date range filtering
- **/admin** — General settings, badge branding, email config, dropdown options
- **/badge/:id** — Printable visitor badge (4"x2.4" thermal label format)

## Features

- [x] SQLite schema with WAL journaling (crash-safe)
- [x] Full CRUD for hosts, visitors, and visits
- [x] HTMX dashboard with 30-second auto-refresh
- [x] Pre-registration workflow
- [x] Walk-in workflow with email notifications
- [x] Group visit registration with bulk badge printing
- [x] Approve / Deny / Check-In / Check-Out / Reschedule / Late actions
- [x] Bulk "Check Out All" for end-of-day
- [x] Searchable visitor log with date filtering
- [x] Areas-of-access tracking (Studios, MCR, Rack Room, etc.)
- [x] Visitor photo capture via webcam
- [x] Thermal badge printing (Brother QL-820NWB, 4"x2.4" labels)
- [x] Customizable badge branding (colors, logo, fonts, escort flag)
- [x] Role-based access control (front desk vs admin)
- [x] Session-based auth with argon2 password hashing
- [x] Admin panel (general settings, badge config, email, dropdowns)
- [x] Microsoft Graph calendar integration (optional)
- [x] Email notifications via Graph API (host arrival, visitor confirmation)
- [x] Kiosk JSON API with shared secret auth
- [x] XSS protection (HTML escaping on all user-controlled output)
- [x] Sanitized error messages (no DB internals exposed to users)
- [x] Responsive layout (desktop + tablet)
- [x] Rescheduled visit auto-promotion (pending when date arrives)

## Security

- Argon2 password hashing (salted, timing-safe)
- HttpOnly session cookies with SameSite=Lax
- Secure flag auto-applied behind HTTPS
- HTML escaping on all user-controlled template output
- Parameterized SQL queries (no SQL injection)
- Kiosk API protected by shared secret header
- Internal error details logged server-side, not exposed to users
- Photo path traversal protection

## Tech Stack

| Layer | Tech | Why |
|---|---|---|
| Backend | Axum (Rust) | Fast, safe, single binary |
| Database | SQLite + WAL | Offline-first, zero ops |
| Frontend | HTMX + plain CSS | No JS build, no npm |
| Templates | Inline Rust strings | No template engine dependency |
| Auth | Argon2 + sessions | Industry-standard password hashing |
| Email | Microsoft Graph API | Enterprise O365 integration |
| Calendar | Microsoft Graph API | Shared mailbox calendar events |

## Deployment

### Local

```bash
cargo run
```

### Cloudflare Tunnel (temporary)

```bash
cloudflared tunnel --url http://localhost:3006
```

### Cloudflare Tunnel (named, persistent)

```bash
cloudflared tunnel login
cloudflared tunnel create gatekeeper
cloudflared tunnel route dns gatekeeper gatekeeper.yourdomain.com
cloudflared tunnel run --url http://localhost:3006 gatekeeper
```

## License

Internal use — WBBH Engineering
