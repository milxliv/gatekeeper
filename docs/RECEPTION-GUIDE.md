# Reception Desk — User Guide

GateKeeper is the visitor management application you use at the front
desk. Use it to check in visitors, print badges, and keep a log of
everyone who came and went.

---

## Getting Started Each Day

1. Open the **GateKeeper** bookmark in your browser (Microsoft Edge or
   Chrome)
2. **If a login screen appears**, enter the password your IT manager
   gave you. (Some installs are set up so you go straight to the
   dashboard with no login — this depends on how IT configured the
   system.)
3. You'll land on the Dashboard

### Starting GateKeeper

GateKeeper runs in the background and starts automatically when the
computer turns on. You don't normally need to do anything to start it.

**If the browser says "can't connect" or "this site can't be reached":**

If GateKeeper runs on this PC (most common setup):
1. Find **gatekeeper.exe** in `C:\GateKeeper\` (or on the desktop)
2. Double-click it
3. A black window will open and stay open — this is normal, just
   minimize it
4. Wait 5 seconds, then refresh the browser

**Do not close the black window** — that will stop GateKeeper. Just
minimize it if it's in your way.

If GateKeeper runs on a server elsewhere on the network and you can't
reach it, contact IT.

### "Not Secure" warning every time?

If your browser shows a security warning every time you open
GateKeeper, the TLS certificate hasn't been trusted on this PC. Tell
IT — they'll import it once and the warning goes away. If you have to
proceed past it temporarily, the camera for visitor photos may not
work until the cert is trusted.

---

## Pre-Visit Registrations (from Hosts)

Hosts will email you a filled-out **Pre-Visit PDF** with visitor details.

### When you receive a pre-visit PDF:

1. Open the PDF
2. In GateKeeper, click **Pre-Register** in the sidebar
3. Copy the information from the PDF into the form:
   - Visitor name
   - Company
   - Host (type the name — it will autocomplete)
   - Purpose
   - Expected date and time
   - Any special notes
4. Click **Submit**
5. **Print the PDF** and file it in the visitor log binder
6. Delete the email from your inbox (PDFs contain personal info)

The visitor now appears on the dashboard under "Upcoming."

---

## Walk-In Visitors

When someone arrives without a pre-registration:

1. Click **Walk-In** in the sidebar
2. Ask for and enter:
   - Their name
   - Their company
   - Who they're visiting (host)
   - Purpose of visit
3. Click **Submit**
4. The camera window opens → take their photo (or click **Skip Photo**)
5. Preview the badge
6. Click **Approve & Print Badge** — the badge prints automatically
7. Hand them the badge
8. Notify the host that their visitor has arrived

---

## Pre-Registered Visitors (When They Arrive)

1. On the Dashboard, find them in today's list (status: "Expected")
2. Click **Check In** on their row
3. Take their photo (or skip)
4. Preview and print the badge
5. Hand them the badge

---

## Group Visits

For a tour group or team of visitors arriving together:

1. Click **Group Visit** in the sidebar
2. Enter the group name (e.g., "Local College Tour")
3. Add each person's name and company
4. Select one host for the group
5. Submit — each person gets their own badge to print

---

## When a Visitor Leaves

- Click **Check Out** next to their name on the Dashboard
- Retrieve their badge (it's valid for today only and should be discarded)
- Their photo is automatically deleted from the system at this moment

### End of Day

- Click **Check Out All** at the bottom of the dashboard to close out any
  visitors who forgot to check out
- All photos for those visitors are deleted at the same time

---

## Looking Up Past Visitors

1. Click **Log** in the sidebar
2. Search by visitor name, company, or host
3. Filter by date range if needed

All check-in and check-out times are recorded automatically.

**Note:** visit records are kept for about 8 hours after the visitor
checks out, then automatically purged for privacy. For longer-term
records, refer to the host's calendar entry or any pre-visit PDF you
filed.

---

## Badge Printing

Badges print on the **Brother QL-820NWB** label printer next to the
computer.

- Badges print in black and red (no other colors)
- Each badge is valid for one day only
- If the badge doesn't print:
  - Check that the printer is powered on
  - Check the label roll isn't empty
  - Press **Ctrl + P** in the badge preview window if the print dialog
    didn't auto-open

---

## Visitor Photos

- Photos are taken with the webcam during check-in
- If the camera window doesn't appear, check the browser's permission
  popup and click **Allow**
- **Photos are deleted from the system the moment you check the visitor
  out** — privacy by default
- You can skip the photo if a visitor declines

---

## Troubleshooting

| Problem | What to do |
|---------|-----------|
| Browser says "can't connect" | If GateKeeper runs on this PC, double-click `gatekeeper.exe` (in `C:\GateKeeper\` or on the desktop). A black window will open — leave it open (minimize if needed). If GateKeeper runs elsewhere on the network, contact IT. |
| Security warning every time | The TLS certificate isn't trusted on this PC — contact IT to install it |
| Camera doesn't appear | Allow camera permission when the browser asks. If you only see a "Not Secure" warning instead, the cert needs to be trusted (see above) |
| Badge prints with red smears | Normal for thermal printing — the app handles this automatically |
| Host name not in the dropdown | Ask Engineering to add the host in the admin panel |
| Visitor isn't in the list | Check if they were pre-registered for a different date, or add them as a walk-in |
| Computer needs to restart | After restart, GateKeeper should start automatically. If not, double-click `gatekeeper.exe` from the desktop |

For anything else, contact **Engineering — Adam Lancaster**.
