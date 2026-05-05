# GateKeeper — Executive Summary

**One-page brief for security and management review.**

---

## What it is

GateKeeper is a self-hosted, internal-network visitor management system that registers visitors, captures a photo, prints a badge, and tracks check-in/check-out. It replaces the paper-and-clipboard sign-in process at a reception desk.

Single binary, Rust + SQLite, no cloud, no subscriptions, no third-party APIs at runtime. MIT-licensed.

---

## Where it runs

Two supported deployment shapes — both stay entirely inside the corporate network:

| Shape | Where the app runs | How users access it |
|---|---|---|
| **A. All-in-one lobby PC** | The reception desk PC itself | Receptionist opens a browser locally on the same PC |
| **B. Server + thin-client browsers** | A server on the company LAN | Authorized internal users browse to `https://<server>:3443/` from any company workstation |

**No internet egress required for visitor flow.** The application makes no outbound HTTP calls in v0.4.0 — no calendar APIs, no email APIs, no third-party services.

---

## Data it touches

- **Visitor name** (required), **company** (optional), **phone/email** (optional), **photo** (optional)
- **Host name + email** (from the staff host list — not visitor PII; this is internal directory data)
- **Visit metadata**: purpose, areas allowed, badge number, timestamps

**Storage location:** local SQLite database file on the host running the binary. Photos: local `photos/` directory.

**No data leaves the host or the corporate network.**

---

## How it's protected

| Control | Implementation |
|---|---|
| Encryption in transit | Native TLS 1.2+ via rustls; both ports HTTPS; HTTP→HTTPS redirect on `:80` |
| Encryption at rest | BitLocker on the host disk (deployment-side requirement; key escrowed to corp Azure / IT key store) |
| Authentication | Argon2-hashed passwords for reception + admin; admin port also requires TOTP MFA with 10 single-use backup codes |
| Authorization | Role-based access (front-desk vs admin), enforced server-side, separated by port |
| Brute-force defense | Login rate limit: 10 attempts per 15-minute sliding window per source IP |
| Input safety | Body cap 16 MB, photo upload validated by file magic bytes (not extension/Content-Type), parameterized SQL throughout |
| PII minimization | Visitor photo unlinked at checkout (default `photo_retention_hours = 0`); visit row + orphan visitor row + photo purged 8 hours after checkout |
| Auditability | Auth failures and source IPs logged via structured logger (`tracing`); dedicated audit-events table is on the v0.5 roadmap |

**v0.4.0 explicitly removed** the Microsoft Graph and email integrations to reduce attack surface and eliminate the need for an external Microsoft tenant, app registration, or client secrets. ~1620 lines of code removed.

---

## Risks (honest list)

| Risk | Mitigation |
|---|---|
| Hand-rolled authentication instead of Entra ID / Okta | TOTP MFA + argon2 + dual-port loopback admin substantially raises the bar; full IdP integration is on the v0.5 roadmap |
| Local SQLite is not natively encrypted | BitLocker on the host disk covers this; SQLCipher could be added in a future release if compliance demands layered defense |
| No queryable audit log table yet | `tracing` log lines exist but are not structured for compliance reporting; audit-events table is a v0.5 work item |
| Self-signed TLS cert by default | Replaceable at any time with a CA-signed cert from the corp PKI by dropping the PEMs at `tls/cert.pem` / `tls/key.pem` |

---

## What's being asked

A go/no-go decision on whether GateKeeper v0.4.0 may be deployed to the lobby PC under the corporate security policy. If gaps are blocking, please specify which so they can be prioritized for v0.4.1 / v0.5.

**Reference docs:**
- Detailed line-by-line security review: `docs/SECURITY-REVIEW.md`
- Operations / setup guide: `docs/IT-SETUP.md`
- Source: `github.com/milxliv/gatekeeper`, tag `v0.4.0`
- License: MIT

---

*Adam Lancaster — Engineering*
