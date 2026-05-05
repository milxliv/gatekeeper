# GateKeeper — Visitor Management System

**Self-hosted, offline-capable visitor management for any small to mid-size business.**

Zero subscriptions. Zero npm. Zero cloud dependency. One Rust binary + SQLite.

Built originally for a broadcast lobby; works for any front desk that needs to register, badge, and log visitors.

## Quick Start

```bash
# Build (requires Rust 1.75+)
cargo build --release

# Configure (copy and edit)
cp .env.example .env

# Run
./target/release/gatekeeper
# → https://localhost:3443 (reception) + https://127.0.0.1:3444 (admin)
```

A self-signed TLS cert is auto-generated on first run at `tls/cert.pem` and `tls/key.pem` (covering `localhost`, `127.0.0.1`, and the machine's hostname). Browsers will show a warning until the cert is trusted (see Deployment for the Windows cert-trust steps); to use a CA-signed cert from your organization, just drop the PEM files in at the same path and restart.

## Configuration

Environment variables (set in `.env` or shell):

| Variable | Default | Description |
|---|---|---|
| `GATEKEEPER_PASSWORD` | *(none)* | Front desk login password (required for auth) |
| `GATEKEEPER_ADMIN_PASSWORD` | *(none)* | Admin login password (falls back to front desk pw) |
| `GATEKEEPER_KIOSK_SECRET` | *(none)* | API key for kiosk check-in endpoint |
| `GATEKEEPER_DB` | `gatekeeper.db` | Path to SQLite database file |
| `GATEKEEPER_PORT` | `3443` | Reception HTTPS port |
| `GATEKEEPER_ADMIN_PORT` | `3444` | Admin HTTPS port (loopback only, `127.0.0.1`) |
| `GATEKEEPER_HTTP_REDIRECT_PORT` | `80` | Port for the HTTP→HTTPS redirect listener (`0` to disable) |
| `GATEKEEPER_TLS_CERT` | `tls/cert.pem` | Path to TLS cert (auto-generated if missing) |
| `GATEKEEPER_TLS_KEY` | `tls/key.pem` | Path to TLS key (auto-generated if missing) |
| `GATEKEEPER_PHOTOS` | `photos` | Directory for visitor photos and logos |
| `RUST_LOG` | `info` | Log level (error, warn, info, debug, trace) |

Two retention settings live in the DB (admin panel or `settings` table), not env vars:

| Setting key | Default | Description |
|---|---|---|
| `photo_retention_hours` | `24` | Hours to retain visitor photos before sweep deletes them |
| `visit_retention_hours` | `8` | Hours after check-out before the visit row + orphan visitor are purged (Path A minimization) |

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
│  ├── Admin Panel      — settings, badge branding, dropdowns  │
│  └── Badge Printing   — thermal badge generation (4"x2.4")  │
└─────────────────────────┬────────────────────────────────────┘
                          │ HTTPS (TLS 1.2+, rustls; HTTP→HTTPS 308 on :80)
┌─────────────────────────┴────────────────────────────────────┐
│  Axum (Rust) — dual-port                                      │
│  ├── Reception :3443    → 0.0.0.0, front-desk LAN access      │
│  ├── Admin     :3444    → 127.0.0.1 loopback only             │
│  ├── Auth middleware    → session + argon2, role-aware, TOTP  │
│  ├── Rate limit         → 10 login attempts / 15 min per IP   │
│  ├── Body cap           → 16 MB max request body              │
│  ├── Photo upload       → magic-byte validated (PNG/JPEG/WebP)│
│  ├── Kiosk API          → JSON endpoint for tablet check-in   │
│  └── SQLite (WAL mode)  → crash-safe, retention-pruned        │
└──────────────────────────────────────────────────────────────┘
```

## Authentication

Two-tier role system:

- **Front desk** (`GATEKEEPER_PASSWORD`) — Dashboard, check-in/out, pre-register, walk-in, group visits, visitor log
- **Admin** (`GATEKEEPER_ADMIN_PASSWORD`) — All of the above plus settings, badge branding, host management, dropdown configuration

If only `GATEKEEPER_PASSWORD` is set, it grants admin access. If neither is set, the app runs without authentication (dev mode).

Sessions use HttpOnly cookies with argon2-hashed passwords. The `Secure` flag is applied automatically when behind HTTPS (always the case in v0.3.0+).

### Admin MFA (TOTP)

The admin port (`/login` on `:3444`) requires both the admin password and a TOTP code from Microsoft Authenticator, Google Authenticator, Authy, or any RFC 6238-compatible app. On the first admin login, GateKeeper generates a TOTP secret + 10 single-use **backup codes** and shows them once on the setup page — print or copy them somewhere safe. They are argon2-hashed and never recoverable from the DB after that page.

At login, the same code field accepts either:
- A 6-digit TOTP code from the authenticator, or
- An 8-character backup code (`xxxx-xxxx`) — case-insensitive, dashes/spaces optional.

Backup codes are marked consumed on use; a warning is logged with the remaining count.

### Recovering a lost authenticator

If the admin loses both their phone and their backup codes, an engineer with shell access to the host can reset MFA:

```
# Stop the GateKeeper service first.
sqlite3 gatekeeper.db <<SQL
DELETE FROM settings WHERE key = 'totp_secret';
DELETE FROM totp_backup_codes;
SQL
# Restart the service. The next admin login generates a fresh secret + new backup codes.
```

This is the documented recovery path; it requires physical/console access and does not depend on email or SMS.

## Visit Workflow

```
Pre-Registered Path:
  Host pre-registers visitor → Visitor arrives → Front desk confirms
  → Host notified → Check in → Photo + Badge → Check out

Walk-In Path:
  Unknown visitor arrives → Front desk enters info
  → Receptionist notifies host → Host approves/denies
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
- **/admin** — General settings, badge branding, dropdown options
- **/badge/:id** — Printable visitor badge (4"x2.4" thermal label format)

## Features

- [x] SQLite schema with WAL journaling (crash-safe)
- [x] Full CRUD for hosts, visitors, and visits
- [x] HTMX dashboard with 30-second auto-refresh
- [x] Pre-registration workflow
- [x] Walk-in workflow with host approval gate
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
- [x] Admin panel (general settings, badge config, dropdowns)
- [x] Kiosk JSON API with shared secret auth
- [x] XSS protection (HTML escaping on all user-controlled output)
- [x] Sanitized error messages (no DB internals exposed to users)
- [x] Responsive layout (desktop + tablet)
- [x] Rescheduled visit auto-promotion (pending when date arrives)

## Security

- **Native TLS (rustls)** on both reception and admin ports — no plaintext on the wire. Auto-generated self-signed cert on first run; replaceable with a CA-signed cert at the same path.
- **HTTP→HTTPS redirect** on `:80` (308) so cleartext URLs land on the secure port.
- **Admin port loopback-only** (`127.0.0.1:3444`) — never reachable from the LAN; you must be on the HP mini console (or use SSH port-forwarding) to admin.
- **Argon2 password hashing** for both reception and admin passwords (salted, timing-safe).
- **TOTP MFA on admin port** with 10 single-use backup codes; password+TOTP both required. See [Admin MFA](#admin-mfa-totp).
- **Login rate limit** — 10 attempts per 15-minute sliding window, per source IP, on each `/login` (reception + admin separately namespaced). Successful logins do not consume budget.
- **HttpOnly + Secure session cookies** with SameSite=Lax.
- **Request body cap** at 16 MB on both routers (DefaultBodyLimit).
- **Photo upload validated by file magic bytes** (`infer` crate) — only PNG / JPEG / WebP accepted; extension and `Content-Type` headers are not trusted.
- **Photo path-traversal protection** on `/photos/:filename`.
- **Parameterized SQL queries everywhere** (`rusqlite ?` placeholders).
- **HTML escaping** on all user-controlled template output (XSS defense).
- **Sanitized error responses** — `rusqlite` errors are logged server-side via `tracing::error!`; users see a generic retry message.
- **Kiosk API** protected by `X-Kiosk-Secret` shared-secret header.
- **Aggressive PII minimization** — visitor photos are unlinked from disk **at the moment the visitor checks out** (default `photo_retention_hours = 0`); the visit row itself + any orphaned visitor row + photo are purged ~8 hours after check-out by the background sweep (`visit_retention_hours`). No long-term local audit trail by design — if your business needs longer retention for compliance, raise the two retention settings in the admin panel.

## Tech Stack

| Layer | Tech | Why |
|---|---|---|
| Backend | Axum (Rust) | Fast, safe, single binary |
| Database | SQLite + WAL | Offline-first, zero ops |
| Frontend | HTMX + plain CSS | No JS build, no npm |
| Templates | Inline Rust strings | No template engine dependency |
| Auth | Argon2 + sessions | Industry-standard password hashing |

## Deployment

GateKeeper supports two physical layouts. The code is the same; only where the binary runs differs.

### Layout A — All-in-one lobby PC

Reception PC runs the GateKeeper binary, has the USB camera and badge printer attached, and the receptionist uses Edge/Chrome on the same machine to drive it. Simplest install. No network hop. Cert is trusted on `localhost` automatically.

### Layout B — Server + thin-client reception PC (recommended for multi-receptionist or remote-admin environments)

GateKeeper runs on a server somewhere on the LAN (a NUC in a rack, a Windows mini in IT, a VM, anything). The reception PC is just a browser pointed at `https://<server>:3443/`. The reception PC's USB camera and badge printer are accessed via the browser (`getUserMedia()` for camera, browser print dialog for printer) — no GateKeeper code runs locally on the reception PC.

This means:

- **No PII at rest on the reception PC ever.** The DB and photo files live on the server, not the lobby workstation.
- **Multiple reception PCs** can hit the same server (busy lobbies, multiple entrances).
- **Server can be hardened/admin-managed separately** from the public-facing reception machine.

Both layouts work the same operationally. Layout B requires the reception PC to trust the server's TLS cert (otherwise `getUserMedia()` is blocked) — push the cert via GPO/MDM or import manually.

### Local development

```bash
cargo run
# → https://localhost:3443 (reception), https://127.0.0.1:3444 (admin)
```

### Windows lobby PC (Layout A or as the reception thin-client in Layout B)

Tested on Windows 11 Pro mini PCs. The same steps apply on Windows Server.

1. **Enable BitLocker** on the system drive with the recovery key escrowed to your IT system (Azure / AD / your password manager). This is the encryption-at-rest control for the local SQLite DB and photos directory.
2. (Optional but recommended) **Enroll the device** in your MDM (Intune, Jamf-for-Windows, etc.) for remote management and remote wipe.
3. Drop the GateKeeper binary in `C:\Program Files\GateKeeper\gatekeeper.exe` and create a working dir at `C:\ProgramData\GateKeeper\`.
4. **Trust the auto-generated self-signed cert** in the Windows certificate store (required so Edge/Chrome will allow `getUserMedia()` for the USB camera). From elevated PowerShell:

   ```powershell
   Import-Certificate -FilePath "C:\ProgramData\GateKeeper\tls\cert.pem" `
                      -CertStoreLocation Cert:\LocalMachine\Root
   ```

   Or push the cert via GPO/MDM so it lands silently. Replacing `tls\cert.pem` and `tls\key.pem` with a CA-signed cert from your organization at any time avoids the trust step.
5. **Firewall**: allow inbound on 443/3443 and 80; deny inbound on 3000/3001 (older non-TLS ports).
6. Register as a Windows service running under a constrained service account (not LocalSystem). Service auto-starts on boot.
7. Receptionist double-clicks a desktop shortcut to `https://localhost:3443/`.

### Cloudflare Tunnel (named, persistent — optional)

```bash
cloudflared tunnel login
cloudflared tunnel create gatekeeper
cloudflared tunnel route dns gatekeeper gatekeeper.yourdomain.com
cloudflared tunnel run --url https://localhost:3443 gatekeeper
```

Cloudflare Tunnel terminates TLS at the edge; the local TLS cert covers the loopback hop.

## License

[MIT](LICENSE) — Copyright © 2026 Adam Lancaster.

You're free to use, modify, and redistribute GateKeeper, including for commercial use, as long as the copyright notice and license remain intact. No warranty.
