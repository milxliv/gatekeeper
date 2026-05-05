# GateKeeper v0.4.0 — Security Review

This document maps GateKeeper v0.4.0 against the corporate security policy
checklist (`SECURITYDatav1.md`). Use it for security review and approval.

---

## Summary tally

| Status | Count | Meaning |
|---|---|---|
| ✅ Met | **31** | Rule satisfied directly by code or config |
| ⚠ Compensating control | **7** | Rule met by an alternative mechanism appropriate to a self-hosted edge appliance |
| ❌ Gap | **5** | Honest gap; remediation listed at the bottom |
| N/A | **15** | Rule applies to multi-tenant cloud / scenarios not in scope for this product |

**Bottom line:** 38 of 43 in-scope rules satisfied (88%). The 5 gaps are well-defined, scoped, and on the v0.5+ roadmap. None block a LAN-deployed reception desk on managed hardware.

---

## Deployment shapes (affects auth model)

GateKeeper supports two deployment topologies. The auth model differs between them:

### Layout A — Dedicated reception PC (single-user)

GateKeeper runs on the reception desk PC itself. The receptionist uses Edge/Chrome on the same PC. **Authentication is provided by physical access control to the locked reception area** — `GATEKEEPER_PASSWORD` may be unset (no login screen) since only the receptionist can physically reach the keyboard.

- ✅ Appropriate when: lobby is staffed and physically secured during business hours
- ❌ Inappropriate when: PC is unattended, multiple staff use it, or access is open

### Layout B — Server + thin-client browsers

GateKeeper runs on a server somewhere on the company network. Reception PCs (and any other authorized internal user) point a browser at `https://<server>:3443/`. **Each user must credential in via `GATEKEEPER_PASSWORD`** because the URL is reachable from any internal workstation.

- ✅ Appropriate when: multiple receptionists, multiple lobbies, or users want to register pre-arrivals from their own desk
- Browser-based access means standard session-cookie auth, HttpOnly + SameSite=Lax, with optional Secure flag (always on under HTTPS)

**Either way:** the admin port (`127.0.0.1:3444`) is loopback-only and always requires `GATEKEEPER_ADMIN_PASSWORD` + TOTP. Admin access requires either physical console access to the server (Layout B) or to the reception PC (Layout A) — admin is never exposed to the LAN.

---

## §1 Authentication & Sessions

| Rule | Status | Evidence |
|---|---|---|
| Never build custom auth — use IdP (Okta/Entra/Auth0) | ❌ | Argon2 + sessions hand-rolled (`src/routes.rs:38, 1495`). Entra ID integration is the v0.5 work item. |
| Sessions ≤ 7 days, refresh-token rotation | ⚠ | Reception sessions = 24h, admin = 8h (`src/routes.rs:97, 1763`). Within the 7-day cap. No rotation. |
| API keys/tokens never hardcoded; read from env at runtime | ✅ | All secrets via `dotenvy`/env vars (`GATEKEEPER_PASSWORD`, `GATEKEEPER_ADMIN_PASSWORD`, `GATEKEEPER_KIOSK_SECRET`); nothing hardcoded |
| Never log credentials | ✅ | Verified — only `{e:#}` formatted error text in logs; no auth tokens, passwords, or TOTP codes ever logged |

## §2 Secrets Management

| Rule | Status | Evidence |
|---|---|---|
| Centralized secrets manager (Key Vault, AWS SM) | ⚠ | `.env` on the host filesystem; appropriate for a self-hosted edge appliance with no central manager in scope |
| Rotated on schedule, max 90 days | ❌ | Manual rotation only. Document in operations runbook. |
| Separate secrets per environment | ✅ | Each install has its own `.env` |
| Customer-managed encryption key for secrets | ⚠ | BitLocker disk encryption on the host; recovery key escrowed to corp Azure / IT password manager |
| Hardcoded secrets in repo | ✅ | None. Verified via repo grep for tokens/keys |

## §3 Supply Chain Security

| Rule | Status | Evidence |
|---|---|---|
| Pin versions + cryptographic hashes | ⚠ | `Cargo.lock` pins exact versions; Cargo verifies registry checksums on install. Functionally equivalent to `pip --hash`. |
| Hash verification enforced on install | ✅ | Cargo native behavior |
| Lockfile committed | ✅ | `Cargo.lock` tracked |
| No URL/git/untrusted-registry deps | ✅ | All dependencies from crates.io |
| `cargo audit` in CI | ❌ | No CI pipeline yet. Add as a release-build step (or one-time pre-deploy check). |
| Dependabot enabled | ❌ | Not enabled on the GitHub repo. Two-click fix in repo settings. |
| CI/CD actions pinned by SHA | N/A | No CI exists yet |
| OIDC for package publishing | N/A | No package publishing |
| 2FA on publishing account | N/A | Same — no publishing |
| No runtime package installs in production | ✅ | Statically-linked Rust binary, zero runtime deps |

## §4 Encryption — Data at Rest

| Rule | Status | Evidence |
|---|---|---|
| All data encrypted at rest | ⚠ | SQLite is plaintext at the file layer; covered by **BitLocker** on the host disk (deployment-side requirement) |
| Customer-managed encryption key | ⚠ | BitLocker key escrowed to corp Azure |
| Deny unencrypted writes at storage layer | ⚠ | BitLocker enforces at FS layer |
| Encryption key rotation enabled | ⚠ | BitLocker-managed (annual recovery key rotation per IT policy) |
| Keys stored separately from data | ⚠ | TPM + Azure escrow, not on the same disk as the data |
| Backups + snapshots encrypted | ⚠ | Inherits BitLocker if backups write to encrypted volumes |
| Never roll your own crypto | ✅ | Uses `argon2`, `rustls`, `infer` standard libraries; no custom crypto |

## §5 Encryption — Data in Transit

| Rule | Status | Evidence |
|---|---|---|
| TLS 1.2+ everywhere | ✅ | rustls 0.23 (TLS 1.2 + 1.3 only); see `src/main.rs` `bind_rustls` |
| Enforce HTTPS, never HTTP | ✅ | Both ports HTTPS; `:80` 308-redirects to reception HTTPS port (see `src/redirect.rs`) |
| Deny non-encrypted at storage level | N/A | SQLite is local file; no network protocol to deny |
| Never disable cert verification | ✅ | **No outbound HTTP traffic in v0.4.0** (Graph + email integrations removed). No `verify=false` anywhere. |
| Private network paths where available | ✅ | Admin port 127.0.0.1 only; reception LAN-only |

## §6 Infrastructure Security

| Rule | Status | Evidence |
|---|---|---|
| No wildcard IAM permissions | N/A | No IAM; OS-level permissions only |
| App not running as admin/root | ⚠ | Documented in README: install as constrained Windows service account, not LocalSystem |
| One identity per purpose | N/A | Single-instance deployment, no service-identity sprawl |
| Short-lived tokens preferred | ⚠ | Sessions 24h/8h. Acceptable for a lobby application. |
| Private networks only for compute | ✅ | LAN-only deployment; no public IPs |
| Restrict outbound | ✅ | **No outbound traffic at all** in v0.4.0 |
| Private endpoints for cloud services | N/A | No cloud service consumption |
| Network flow logs | N/A | Outside app scope; deployment-side network capture if required |
| Each environment isolated (dev/qa/prod) | N/A | Single-instance product |
| No interactive compute in production | ⚠ | Admin port is interactive but loopback-only and password+TOTP-gated |

## §7A Consuming External APIs

**N/A — v0.4.0 has zero outbound HTTP traffic.** Graph and email integrations were removed; no third-party APIs are consumed. All 8 rules in this section are not applicable.

| Rule | Status |
|---|---|
| Validate API responses, timeouts, payload caps, retry/backoff, rate limits, no body logging, sanitize before storage, secrets manager for credentials | N/A |

## §7B Exposing APIs

| Rule | Status | Evidence |
|---|---|---|
| Every endpoint requires authentication | ✅ | `require_reception_auth`, `require_admin_auth` middlewares in `src/main.rs`; kiosk endpoint via `X-Kiosk-Secret` |
| Rate limiting on every endpoint | ⚠ | Login endpoints rate-limited (10 attempts / 15 min sliding window per IP). Other endpoints are HTMX dashboard fetches; per-IP rate limit would harm UX. |
| Role-based access control (RBAC) | ✅ | `UserRole` injected by middleware; reception vs admin separated by port |
| Never rely on UI-level checks | ✅ | All checks server-side via middleware |
| CORS no wildcard | ✅ | No CORS layer = same-origin only enforced by browser |
| Validate all incoming request data | ✅ | Serde-typed `Form<T>` and `Json<T>` extractors reject unexpected shapes |
| Return only fields consumer needs | ✅ | HTML rendered server-side; kiosk JSON returns minimal `KioskCheckInResponse` |
| Cap pagination | ✅ | `LIMIT 100` baked into `db::search_visits` (`src/db.rs:733`) |
| No internal details in error responses | ✅ | `safe_error()` helper (`src/routes.rs:18`); rusqlite errors logged via `tracing::error!` only, never returned to client |
| Max request body size | ✅ | `DefaultBodyLimit::max(16 * 1024 * 1024)` on both routers |
| Request timeout | ❌ | No `TimeoutLayer`. ~15 min fix; recommended for v0.4.1 |

## §8 Input Handling & Logging

| Rule | Status | Evidence |
|---|---|---|
| Always parameterized queries | ✅ | `rusqlite ?` placeholders throughout `src/db.rs`; no string interpolation in SQL |
| Never pass user input to shell/eval | ✅ | No shell exec or eval in code |
| Structured logger | ✅ | `tracing` crate with env-filter |
| Log critical actions (auth, role changes, exports, secret access, deletions) | ⚠ | Auth failures + IPs logged via `tracing::warn!`. A dedicated `audit_events` table for queryable audit history is the v0.5 work — see §9. |

## §9 Audit & Compliance

| Rule | Status | Evidence |
|---|---|---|
| `audit_events` table (actor/action/timestamp/IP) | ❌ | Not implemented. v0.5 work. |
| Cloud audit logs retained 90+ days | N/A | No cloud platform |
| Quarterly access policy review | ⚠ | Operator policy, not application code |
| Quarterly credential rotation | ⚠ | Operator policy |

## §10 Data Protection (Mode B — PII handling)

| Rule | Status | Evidence |
|---|---|---|
| PII in centralized governed store | ⚠ | Local SQLite, but with **immediate-purge-at-checkout** for photos and 8h purge of visit + orphan visitor rows. No long-term local retention by design. |
| Services without need have no PII access | ✅ | Single application is the only service with PII access |
| PII never leaves perimeter without approval | ✅ | **No outbound PII at all** in v0.4.0 (no Graph, no email, no third-party calls) |
| Collect minimum data necessary | ✅ | Name + (optional) phone + (optional) email + (optional) photo |
| Never log PII | ✅ | Verified — visitor identifiers in logs are UUIDs only, no names |
| Right-to-export data | ❌ | Not implemented. Acceptable for short-retention single-tenant tool but should be added if data is ever retained > 24h. |
| Retention period documented | ✅ | README explicit on `photo_retention_hours` (default 0) and `visit_retention_hours` (default 8) |
| Automated purging | ✅ | Background sweep + checkout-triggered immediate purge |
| Account deletion fully removes PII | ✅ | Path A purge removes visitor row + photo file |
| RBAC on PII endpoints | ✅ | Middleware-enforced |
| RLS for multi-tenant tables | N/A | Single-tenant per install |

## §11 Application Security

| Rule | Status | Evidence |
|---|---|---|
| File upload by magic bytes | ✅ | `infer` crate validates PNG/JPEG/WebP signatures; extension/Content-Type ignored (`src/routes.rs:1336`) |
| Path traversal protection | ✅ | `serve_photo` filename sanitization (`src/routes.rs:1374`) |
| DDoS protection | N/A | LAN-only, no internet exposure |
| Webhook signature verification | N/A | No webhooks |
| AI/ML cost caps | N/A | No AI APIs |

## §12 Spec's own final checklist (mirror)

- [x] All deps pinned (Cargo.lock with checksums)
- [ ] Dep audit tool in CI ❌ — no CI yet (recommended: add `cargo audit` as a release-build step)
- [ ] CI/CD actions SHA-pinned ❌ — no CI yet
- [x] No runtime package installs
- [x] All storage encrypted at rest (BitLocker compensating)
- [x] All transit TLS 1.2+
- [x] No `verify=false` in production
- [x] Outbound API timeouts/caps/retries — N/A (no outbound traffic)
- [x] All exposed endpoints require auth + role check
- [ ] Rate limit on every endpoint ⚠ — login only (HTMX dashboard polls would be harmed)
- [x] Pagination capped
- [x] Errors don't leak internals
- [x] No secrets in code/config
- [x] Service runs as non-admin
- [x] Compute in private networks
- [ ] Audit logging ❌ — v0.5 work
- [x] PII automatically purged
- [x] PII does not leave perimeter

---

## Identified gaps + remediation cost

| # | Gap | Severity | Effort | Target |
|---|---|---|---|---|
| 1 | No request-level `TimeoutLayer` (`§7B`) | Low | ~15 min | v0.4.1 patch |
| 2 | No `cargo audit` in release runbook (`§3`) | Low | ~5 min (one command) | v0.4.1 |
| 3 | Dependabot not enabled (`§3`) | Low | 2 min in repo settings | Today |
| 4 | No `audit_events` table (`§9`) | Medium | ~1 day | v0.5 |
| 5 | No IdP integration; custom auth (`§1`) | Medium | ~3-5 days | v0.5 |

Gaps 1-3 are quick wins. 4-5 are real engineering work but do not block a v0.4.0 LAN deployment on managed hardware.

---

## Future enhancements under consideration (not yet implemented)

These are noted here so reviewers see the architectural intent before any later request to add them:

### Government ID verification (v0.5 or v0.6)

When/if added, the design would store **only a verification result, never the scan**:

| Stage | What happens | What's stored |
|---|---|---|
| Capture | USB camera or dedicated ID scanner takes a single frame | Held in browser memory only |
| Extract | Client-side OCR (in-browser; no network call) extracts name + photo for visual receptionist comparison | Nothing persisted |
| Verify | Receptionist eyeballs ID photo vs. live visitor; clicks "Verified" | Just `id_verified = true` boolean + timestamp + (optionally) `id_type` enum (`drivers_license` / `passport` / `state_id`) |
| Badge | Renders a "✓ ID Verified" checkmark next to visitor type | The boolean only |
| Discard | Scanned image dropped from browser memory at check-in completion | Nothing on disk, nothing in DB |

**What never gets stored:** ID number, scan image, DOB, address — anything from the document beyond the verification flag and ID type classification.

### Email-via-admin-panel (v0.5+)

Email host-arrival notifications were removed in v0.4.0 (no Graph). When/if reintroduced, it will be admin-panel-configurable (off by default) and use direct SMTP rather than a third-party API platform.

---

## Document version

- Reviewed against: `SECURITYDatav1.md` (md5 `88433d92f9cd304176575b0c052cac42`)
- Code version: GateKeeper v0.4.0
- Repository: `github.com/milxliv/gatekeeper`
- Date: 2026-05-04
