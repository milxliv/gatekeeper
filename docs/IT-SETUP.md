# IT Setup — GateKeeper v0.4.0

Setup and verification steps for installing GateKeeper.

**Hardware (typical):** HP mini, Windows 11 Pro
**Install location:** `C:\GateKeeper\`

---

## 0. Pick a deployment layout first

GateKeeper supports two physical layouts. Pick one before starting.

### Layout A — All-in-one lobby PC (simplest)

GateKeeper runs on the reception desk PC itself. The receptionist uses
Edge/Chrome on the same PC. USB camera and badge printer attached
locally. **`GATEKEEPER_PASSWORD` may be left unset** if the lobby is
physically secured during business hours.

### Layout B — Server + thin-client reception PC

GateKeeper runs on a server somewhere on the company LAN. The reception
PC is just a browser pointed at `https://<server>:3443/`. **Each user
must credential in** via `GATEKEEPER_PASSWORD`. The reception PC's USB
camera and badge printer are accessed through the browser; no
GateKeeper code runs on the reception PC.

The rest of this document covers both layouts. Where steps differ,
they're labeled. **A** = lobby PC, **B** = server + thin-client.

---

## 1. Confirm Files Are in Place

`C:\GateKeeper\` should contain:

- `gatekeeper.exe` — the application
- `.env` — configuration file
- `gatekeeper.db` — database (created on first run)
- `photos\` — visitor photo directory (created on first run)
- `tls\cert.pem` and `tls\key.pem` — TLS certificate (auto-generated on
  first run; or replace with a corp-CA-signed cert at any time)

---

## 2. Enable BitLocker (encryption at rest)

GateKeeper's local SQLite database and photo directory contain visitor
PII. BitLocker on the system drive provides the encryption-at-rest
control.

1. Settings → Privacy & security → Device encryption (or Manage
   BitLocker for Pro editions)
2. Turn on BitLocker for the system drive
3. **Escrow the recovery key to corp Azure / your IT password manager.**
   Do not save the recovery key in `C:\GateKeeper\` or anywhere on the
   protected disk
4. Verify status: `manage-bde -status C:` should show `Encryption
   Method: XTS-AES 128` or stronger and `Conversion Status: Fully
   Encrypted`

If the server is Intune-managed, the recovery key escrows to Azure
automatically.

---

## 3. Auto-Start on Boot

So the application doesn't need to be launched manually after a reboot:

1. Press **Win + R**, type `shell:startup`, press Enter
2. In the Startup folder, right-click → **New > Shortcut**
3. Location: `C:\GateKeeper\gatekeeper.exe`
4. Click Next, name it **GateKeeper**, click Finish
5. Right-click the new shortcut → **Properties**
6. Set **Start in:** `C:\GateKeeper\`
7. Set **Run:** `Minimized`
8. Click OK

For a more robust install, register `gatekeeper.exe` as a Windows
service running under a constrained service account (not LocalSystem).
NSSM or `sc create` work; example below:

```
sc create GateKeeper binPath= "C:\GateKeeper\gatekeeper.exe" start= auto obj= "NT Service\GateKeeper"
```

**Test:** reboot the PC. After login, the application should be
listening on `https://localhost:3443` within ~5 seconds.

---

## 4. Power Settings (Never Sleep)

The host PC must stay awake so GateKeeper is always available.

1. Settings → System → Power & battery
2. Screen: turn off after 10 minutes (optional)
3. Sleep: **Never** (when plugged in)
4. Under Power Mode, select **Best performance** (optional)

---

## 5. Trust the GateKeeper TLS Certificate

GateKeeper auto-generates a self-signed TLS cert on first run. Browsers
must trust this cert, otherwise:

- The receptionist sees a "Not Secure" warning every time
- **`getUserMedia()` is blocked**, which means the USB camera will not
  work for visitor photos

### Layout A (lobby PC) — install on the reception PC

From an elevated PowerShell:

```powershell
Import-Certificate -FilePath "C:\GateKeeper\tls\cert.pem" `
                   -CertStoreLocation Cert:\LocalMachine\Root
```

### Layout B (server + thin-client) — install on every reception PC that will use the system

Copy `tls\cert.pem` from the server to each reception PC, then run the
same `Import-Certificate` command on each. Or push via GPO/MDM so it
lands silently across the fleet.

### Better long-term — replace with a corp-CA-signed cert

If your IT issues internal CA certs, drop the PEM files at
`C:\GateKeeper\tls\cert.pem` and `C:\GateKeeper\tls\key.pem` and
restart GateKeeper. Browsers that already trust the corp CA will trust
the cert silently — no per-PC import needed.

---

## 6. Browser Setup

1. Set the default browser to **Microsoft Edge** or **Chrome**
2. **Layout A:** open `https://localhost:3443` and bookmark as the home
   page
3. **Layout B:** open `https://<server-hostname>:3443/` and bookmark as
   the home page
4. When prompted, allow **camera** access (required for visitor photos)
5. The first visit will show a security warning until step 5 above is
   completed

---

## 7. Printer Setup

1. Plug the **Brother QL-820NWB** thermal printer into USB on the
   reception PC (Layout A *or* B — the printer always lives on the
   reception PC, not the server)
2. Install the Brother driver from
   [Brother's website](https://support.brother.com) if not already
   installed
3. Load a DK-22214 continuous label roll (or equivalent 2.4" wide)
4. In Windows printer settings, make sure the Brother printer is set
   as the default — or at least easily selectable from the browser's
   print dialog

**Test print:**

- In GateKeeper, create a test walk-in visitor
- Click Print Badge on the preview screen
- Verify output prints correctly

---

## 8. Firewall

### Layout A (lobby PC)

The reception PC only needs the application reachable from itself.

- Allow inbound on TCP **443/3443** (HTTPS reception) and **80** (HTTP
  → HTTPS redirect) — only required if anyone else on the LAN should
  reach the dashboard from another PC. If access is purely from the
  same machine via `localhost`, no inbound rules are needed.
- Admin port (3444) is bound to `127.0.0.1` and never reachable from
  the network. No rule needed.

### Layout B (server + thin-client)

The server must be reachable from authorized reception PCs.

- Allow inbound on TCP **3443** (HTTPS) and **80** (redirect) from the
  reception subnet
- Deny inbound on **3000** and **3001** (older non-TLS ports from
  prior versions)
- Admin port (3444) stays loopback-only on the server

---

## 9. Set Strong Passwords + Admin MFA

The admin panel at `https://127.0.0.1:3444` (or the server's loopback)
requires a password and an authenticator code.

### Set a strong admin password

1. Open `C:\GateKeeper\.env` in Notepad
2. Find:
   ```
   GATEKEEPER_ADMIN_PASSWORD=changeme
   ```
   and replace `changeme` with a strong password (16+ chars, mixed
   case, numbers, symbols)
3. Layout B only — set `GATEKEEPER_PASSWORD=...` for the reception
   port too. (Layout A may leave this blank if relying on physical
   access control to the locked reception area.)
4. Save the file
5. Restart `gatekeeper.exe`

### Complete the TOTP setup (first admin login)

1. Open `https://127.0.0.1:3444` (must be at the host's keyboard, not
   over the LAN)
2. Enter the admin password
3. A QR code will appear — scan it with an authenticator app:
   - Microsoft Authenticator
   - Google Authenticator
   - Authy
   - 1Password / Bitwarden built-in TOTP
4. **Save the 10 backup codes shown on the page.** Print them, save
   them to your password manager, or both. **They will not be shown
   again** after this screen closes.
5. Enter the 6-digit code from the authenticator to confirm

After setup, future admin logins require password + (authenticator
code OR an unused backup code).

### Lost authenticator recovery

If the admin authenticator phone is lost AND all 10 backup codes are
lost, an engineer with shell access can reset MFA:

```
sqlite3 C:\GateKeeper\gatekeeper.db ^
  "DELETE FROM settings WHERE key='totp_secret'; ^
   DELETE FROM totp_backup_codes;"
```

Then restart GateKeeper. The next admin login will trigger a fresh
TOTP enrollment with new backup codes.

---

## 10. Photo & Visit Retention (privacy)

GateKeeper minimizes how long visitor PII lives on disk:

- **Photos** are unlinked from disk **the moment a visit checks out**
  (default `photo_retention_hours = 0`)
- **Visit rows** + **orphaned visitor rows** + their photos are purged
  ~8 hours after checkout (`visit_retention_hours = 8`)

Operators who want a longer grace window (e.g. 1h after checkout for
badge reprints) can change either setting in the admin panel under
General Settings.

---

## 11. Verification Checklist

- [ ] BitLocker enabled and recovery key escrowed
- [ ] `gatekeeper.exe` starts automatically after reboot
- [ ] `https://localhost:3443` (or server URL) loads the dashboard
- [ ] No browser security warning (cert imported successfully)
- [ ] Walk-in form submits successfully
- [ ] Webcam captures a photo
- [ ] Badge prints correctly on Brother QL-820NWB
- [ ] Check-in / check-out cycle works end to end
- [ ] After checkout, the visitor's photo file is deleted from
      `C:\GateKeeper\photos\`
- [ ] `https://127.0.0.1:3444` prompts for admin password
- [ ] Admin TOTP is set up and working
- [ ] Admin backup codes are saved somewhere outside the host PC
- [ ] Login rate limit working (try 11 wrong passwords quickly →
      should lock out for 15 minutes)
- [ ] HTTP→HTTPS redirect: `http://localhost/` 308-redirects to
      `https://localhost:3443/`
- [ ] Firewall rules match deployment layout

---

## Support

Contact **Adam Lancaster — Engineering** for any issues during setup.

- Source: `github.com/milxliv/gatekeeper`
- Tag: `v0.4.0`
- License: MIT
- Security review: `docs/SECURITY-REVIEW.md`
- Executive summary: `docs/EXECUTIVE-SUMMARY.md`
