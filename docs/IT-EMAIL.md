Subject: GateKeeper Visitor System — Setup Request for Lobby PC

Hi [IT Manager],

I'd like to get the GateKeeper visitor management system fully set up on
the lobby PC and test it end-to-end. I have the application built, but I
need your help with a few setup items to make it production-ready.

## What's needed

**1. Auto-start on boot**
Add `C:\GateKeeper\gatekeeper.exe` to the Windows Startup folder so it
launches automatically when the PC powers on. Set it to run minimized.

**2. Power settings**
Set the PC to never sleep when plugged in. It needs to stay awake during
business hours so visitors can be checked in.

**3. BitLocker**
Confirm BitLocker is enabled on the system drive with the recovery key
escrowed to corp Azure (or our standard IT key store). This is the
encryption-at-rest control for the local SQLite database and the photos
directory.

**4. Trust the GateKeeper TLS certificate**
GateKeeper auto-generates a self-signed TLS cert on first run at
`C:\GateKeeper\tls\cert.pem`. The Windows certificate store needs to
trust it (otherwise Edge/Chrome will block USB camera access for visitor
photos). From an elevated PowerShell, run:

```
Import-Certificate -FilePath "C:\GateKeeper\tls\cert.pem" `
                   -CertStoreLocation Cert:\LocalMachine\Root
```

If you have a corp CA-signed certificate available, drop the PEM files
at `C:\GateKeeper\tls\cert.pem` and `C:\GateKeeper\tls\key.pem` and
restart instead — that avoids the self-signed-trust step entirely.

**5. Verify printer**
Confirm the Brother QL-820NWB thermal badge printer is installed and
prints a test badge from the application.

**6. Verify camera**
Confirm the browser has camera permission to capture visitor photos at
`https://localhost:3443`.

**7. Firewall**
Allow inbound on TCP 443/3443 (HTTPS) and 80 (HTTP→HTTPS redirect).
Deny inbound on the older non-TLS ports 3000/3001 if any rules from
prior testing remain. The admin port (3444) is bound to `127.0.0.1` and
does not need a firewall rule — it's only reachable from the same PC.

**8. Admin password + authenticator**
Set a strong admin password (replacing the placeholder) in the `.env`
file, and complete the authenticator (TOTP) setup the first time you
log in to the admin panel. GateKeeper will show 10 single-use backup
codes during setup — print or save them somewhere safe; they're the
recovery path if the authenticator phone is lost. The codes will not be
shown again after that screen.

I've attached a detailed setup document with step-by-step instructions
and a verification checklist (`docs/IT-SETUP.md`).

## Context

- v0.4.0 — runs entirely inside the company network, no cloud services,
  no third-party APIs at runtime
- Single executable, no installers needed
- Database and photos stay on the lobby PC (or on a LAN server, depending
  on which deployment shape you prefer — see the setup doc)
- HTTPS by default; admin panel is loopback-only and never exposed to
  the LAN
- MIT-licensed; source at `github.com/milxliv/gatekeeper`

For the corporate security review, I've also attached:
- `docs/EXECUTIVE-SUMMARY.md` — one-page brief
- `docs/SECURITY-REVIEW.md` — line-by-line mapping against the corp
  security policy

Happy to walk through any of this in person. Let me know what time works.

Thanks,
Adam Lancaster
Engineering
