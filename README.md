# ⛊ GateKeeper — WBBH Visitor Management System

**Self-hosted, offline-capable visitor management for broadcast facilities.**

Zero subscriptions. Zero npm. Zero cloud dependency. One Rust binary + SQLite.

## Quick Start

```bash
# Build (requires Rust 1.75+)
cargo build --release

# Run
./target/release/gatekeeper
# → http://localhost:3000
```

## Configuration

Environment variables (all optional):

| Variable          | Default          | Description                    |
|-------------------|------------------|--------------------------------|
| `GATEKEEPER_DB`   | `gatekeeper.db`  | Path to SQLite database file   |
| `GATEKEEPER_PORT` | `3000`           | HTTP port                      |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Browser (HTMX)                                             │
│  ├── Dashboard        — live visitor status, auto-refresh   │
│  ├── Pre-Register     — staff registers expected visitors   │
│  ├── Walk-In          — receptionist logs surprise visits   │
│  ├── Hosts            — manage staff notification targets   │
│  └── Visitor Log      — searchable history + date filter    │
└────────────────────────┬────────────────────────────────────┘
                         │ HTTP (no JS framework, no WebSocket)
┌────────────────────────┴────────────────────────────────────┐
│  Axum (Rust)                                                │
│  ├── Page routes       → full HTML responses                │
│  ├── API routes        → HTMX partial HTML fragments        │
│  ├── Templates         → inline Rust, zero template engine  │
│  └── SQLite (WAL mode) → crash-safe, offline-first          │
└─────────────────────────────────────────────────────────────┘
```

## Visit Workflow

```
Pre-Registered Path:
  Host pre-registers visitor → Visitor arrives → Receptionist confirms
  → Host notified "your visitor is here" → Check in → Badge → Check out

Walk-In Path:
  Unknown visitor arrives → Receptionist enters into system
  → Host gets ALERT (SMS + Email) → Host approves/denies
  → Visitor waits → Check in on approval → Check out
```

## Status Flow

```
pending → approved → checked_in → checked_out
    └──→ denied
```

## Pages

- **/** — Dashboard with today's stats, active visits, upcoming pre-registrations
- **/pre-register** — Form for staff to register expected visitors ahead of time
- **/walk-in** — Front desk form for unannounced visitors (triggers alerts)
- **/hosts** — Add/manage staff who can receive visitor notifications
- **/log** — Full searchable visitor history with date range filtering

## What's Built (Session 1)

- [x] SQLite schema with WAL journaling (crash-safe)
- [x] Full CRUD for hosts, visitors, and visits
- [x] HTMX dashboard with 30-second auto-refresh
- [x] Pre-registration workflow
- [x] Walk-in workflow with notification hooks
- [x] Approve / Deny / Check-In / Check-Out actions
- [x] Searchable visitor log with date filtering
- [x] Areas-of-access tracking (Studios, MCR, Rack Room, etc.)
- [x] Responsive layout (works on desktop + tablet at front desk)

## Roadmap

### Session 2: Notification Engine
- [ ] SMS via Twilio REST API (direct HTTP, no SDK)
- [ ] Email via SMTP relay / Microsoft 365
- [ ] Notification queue with retry logic
- [ ] Host approval via SMS reply or web link

### Session 3: Kiosk Mode
- [ ] Simplified touch-friendly check-in UI
- [ ] Pre-registered visitor lookup by name
- [ ] Thermal badge printing
- [ ] Photo capture (optional)

### Session 4: Hardening
- [ ] Authentication (station login for dashboard access)
- [ ] Audit log export (CSV/PDF for Hearst compliance)
- [ ] Badge numbering system
- [ ] Visitor photo storage
- [ ] Active Directory integration for host list

## Tech Stack

| Layer     | Tech                          | Why                              |
|-----------|-------------------------------|----------------------------------|
| Backend   | Axum (Rust)                   | Fast, safe, single binary        |
| Database  | SQLite + WAL                  | Offline-first, zero ops          |
| Frontend  | HTMX + plain CSS              | No JS build, no npm              |
| Templates | Inline Rust strings           | No template engine dependency    |
| SMS       | Twilio REST (planned)         | Direct HTTP calls from Rust      |
| Email     | SMTP (planned)                | Your existing relay              |

## License

Internal use — WBBH Engineering
