# Installing Edgeway

- [macOS](#macos)
  - [Install with one command (recommended)](#install-with-one-command-recommended)
  - [Install from a downloaded zip](#install-from-a-downloaded-zip)
  - ["Edgeway can't be opened" — and how to never see it again](#edgeway-cant-be-opened--and-how-to-never-see-it-again)
  - [Permissions](#permissions)
  - [First run and pairing](#first-run-and-pairing)
  - [A Mac mini without its own keyboard](#a-mac-mini-without-its-own-keyboard)
  - [Updating and uninstalling](#updating-and-uninstalling)
- [Windows (planned)](#windows-planned)
- [Linux (planned)](#linux-planned)

---

## macOS

Requirements: macOS 13 Ventura or later, on Apple silicon or Intel. Both
computers must be on the same local network (Wi-Fi or Ethernet).

### Install with one command (recommended)

Open Terminal on each Mac and run:

```bash
curl -fsSL https://raw.githubusercontent.com/rafeuddaraj/edgeway/main/scripts/install.sh | bash
```

The installer picks the right download for your Mac (Apple silicon or Intel),
installs Edgeway in `/Applications`, and opens it. **Use the same command for
every update.** Because `curl` doesn't mark files as "downloaded from the
internet", macOS never shows the "can't be opened" warning for Edgeway
installed this way, now or after updates.

To install a zip you already have (for example one you built):

```bash
scripts/install.sh dist/Edgeway-0.1.0-apple-silicon.zip
```

### Install from a downloaded zip

1. Download `Edgeway-<version>-apple-silicon.zip` (M1 and later) or
   `Edgeway-<version>-intel.zip` from the
   [releases page](https://github.com/rafeuddaraj/edgeway/releases).
2. Double-click the zip and drag **Edgeway** into **Applications**.
3. Open it. The first time, macOS will probably stop you, as explained next.

### "Edgeway can't be opened" — and how to never see it again

Apple shows *"Apple could not verify 'Edgeway' is free of malware"* for any
app that wasn't notarized through a paid Apple Developer account. Notarized
releases are planned. Until then, choose **one** of the following.

**A. Clear the download flag once (Terminal):**

```bash
xattr -dr com.apple.quarantine /Applications/Edgeway.app
```

This removes the "downloaded from the internet" mark from this copy of the
app. It has to be repeated only if you later replace the app with a new copy
downloaded through a browser. To avoid that entirely, update with the
one-command installer above, which never sets the mark.

**B. Allow it in System Settings (no Terminal):**

Open Edgeway once and dismiss the warning, then go to
**System Settings › Privacy & Security**, scroll down and click
**Open Anyway** next to the Edgeway message. Confirm with your password.
macOS remembers this decision for that copy of the app.

**C. Build it yourself.** Apps you build on your own Mac are never
quarantined. See [BUILDING.md](BUILDING.md).

> Please don't turn off Gatekeeper for the whole system
> (`spctl --master-disable`) just for Edgeway. It lowers protection for every
> app you download.

### Permissions

Edgeway asks only for what it needs:

| Permission | Where | Why |
|---|---|---|
| **Accessibility** | System Settings › Privacy & Security › Accessibility | On the sharing Mac: to keep your keyboard and trackpad from reaching it while another computer has focus. On the controlled Mac: to type and move the pointer. |
| **Local Network** | Asked automatically the first time | To find your other computers and connect to them directly. |
| **Incoming connections** (only if the macOS firewall is on) | Asked automatically on the controlled Mac | So the sharing Mac can connect. |

Edgeway walks you through Accessibility during setup and shows a warning in
its menu until it is allowed. macOS remembers the permission for as long as
the app's signature stays the same, so updates from releases keep it.

If you build Edgeway yourself, run
`scripts/create-signing-identity.sh` once so your builds share one signature
and macOS stops asking after every rebuild (see
[BUILDING.md](BUILDING.md#keep-permissions-across-rebuilds)).

If the firewall is on and you missed the prompt on the controlled Mac:

```bash
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --add /Applications/Edgeway.app
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --unblockapp /Applications/Edgeway.app
```

### First run and pairing

1. Open Edgeway on both computers. A short setup asks what each Mac does:
   - **Share my keyboard and trackpad**: the Mac you type on (your MacBook).
   - **Be controlled by another computer**: the Mac you want to control
     (your Mac mini).
2. Allow Accessibility on both, and leave **Start Edgeway when you log in**
   on.
3. On the sharing Mac, click the Edgeway icon in the menu bar
   (a pointer crossing an edge). The other Mac appears under **Nearby**.
   Choose **Pair with …**.
4. The controlled Mac shows a six-digit code on its own screen. Type it on
   the sharing Mac. That's the only time you pair. From now on the two
   reconnect by themselves.
5. Push the pointer through the top edge of the sharing Mac's screen. To
   change where the other computer sits, choose **Arrange Displays…** and
   drag it.

Shortcuts on the sharing Mac:

| Shortcut | Action |
|---|---|
| ⌃⌥⌘H | Bring control back to this Mac immediately |
| ⌃⌥⌘P | Pause or resume sharing |

### A Mac mini without its own keyboard

**First-time setup** needs some way to click on the Mac mini once: to open
Edgeway, choose its role and allow Accessibility. Either borrow a keyboard
and mouse for five minutes, or use macOS's built-in Screen Sharing (no Apple
ID required):

1. On the Mac mini: System Settings › General › Sharing › **Screen Sharing** on.
2. On the MacBook: Finder › Go › Connect to Server… › `vnc://mac-mini.local`
   (use the Mac mini's name).

After that, Edgeway takes over and Screen Sharing can be turned off again.

**Coming back after a restart, without touching it.** Edgeway is an app, so
it starts when a user session starts. Choose the setup that fits you:

| Setup | What happens after a restart or power cut |
|---|---|
| **FileVault off + automatic login** (simplest) | The Mac mini logs in by itself, Edgeway starts, and your MacBook reconnects within seconds. |
| **FileVault on + unlock over SSH** (macOS 26 Tahoe, Apple silicon) | Run `ssh you@mac-mini.local` from the MacBook and type your password to unlock the disk. Then the Mac continues booting. Remote Login must be turned on beforehand. |
| **FileVault on, nothing else** | Someone has to type the password on the Mac mini, with a real keyboard, before Edgeway can run. No app can avoid this. |

To set up the first option on the Mac mini:

1. System Settings › Privacy & Security › **FileVault**: Turn Off.
2. System Settings › Users & Groups › **Automatically log in as**: your user.
3. System Settings › Energy (or Battery › Options):
   - **Start up automatically after a power failure**: on
   - **Wake for network access**: on
   - **Prevent automatic sleeping when the display is off**: on
4. Edgeway Settings › General › **Start Edgeway when you log in**: on.

Turning FileVault off means anyone with physical access to the Mac mini can
read its disk. Decide whether that's acceptable where it sits. The details,
including what is and isn't possible at the login window, are in
[LIMITATIONS.md](LIMITATIONS.md).

### Updating and uninstalling

Update by running the installer command again. Your pairings and settings
are kept.

To remove Edgeway completely:

```bash
osascript -e 'tell application id "io.github.rafeuddaraj.Edgeway" to quit'
rm -rf /Applications/Edgeway.app
rm -rf ~/Library/Application\ Support/Edgeway
defaults delete io.github.rafeuddaraj.Edgeway
tccutil reset Accessibility io.github.rafeuddaraj.Edgeway
```

Then remove it from System Settings › General › Login Items if it is still
listed.

---

## Windows (planned)

The Windows version is not released yet. It will use the same core and
protocol, so it will pair with Macs the same way. This is what installing it
will involve, so nothing comes as a surprise.

**"Windows protected your PC" (SmartScreen).** Shown for apps that aren't
signed with a code-signing certificate that has earned reputation. Click
**More info › Run anyway**. To stop Windows from treating the download as
"from the internet" at all, unblock it once before running it:

```powershell
Unblock-File -Path "$env:USERPROFILE\Downloads\Edgeway-Setup.exe"
```

Or right-click the file › **Properties** › tick **Unblock** › OK. Installing
with `winget` (once it is published there) avoids the prompt entirely, much
like the macOS one-command installer.

**Windows Defender Firewall.** The first time the controlled PC starts,
Windows asks whether Edgeway may communicate on networks. Allow **Private
networks**. If you missed it, run this in an administrator PowerShell:

```powershell
New-NetFirewallRule -DisplayName "Edgeway" -Direction Inbound -Action Allow `
  -Program "C:\Program Files\Edgeway\Edgeway.exe" -Profile Private
```

Make sure your home network is set to **Private**, not Public
(Settings › Network & internet › your network › Network profile type).

**Administrator windows and the sign-in screen.** Windows does not let a
normal app control windows that run as administrator, or the sign-in and
UAC screens. The Windows version will include an optional background service
for those; a regular installation works for everything else.

## Linux (planned)

The Linux version is not released yet. It will ship as an AppImage and
native packages and use the same protocol. What to expect:

**Running an AppImage:**

```bash
chmod +x Edgeway-x86_64.AppImage
./Edgeway-x86_64.AppImage
```

On Ubuntu 22.04 and later, AppImages need FUSE 2: `sudo apt install libfuse2`
(on 24.04: `sudo apt install libfuse2t64`).

**Discovery** uses Avahi, which most desktops already run:

```bash
sudo systemctl enable --now avahi-daemon
```

**Firewall** (if `ufw` is enabled), allowing only your local network:

```bash
sudo ufw allow 5353/udp comment 'mDNS'
sudo ufw allow from 192.168.0.0/16 to any port 47820 proto tcp comment 'Edgeway'
```

**Wayland (GNOME 45+, KDE Plasma 6.1+).** The desktop asks once whether
Edgeway may "control the keyboard and pointer" or "capture input". Choose
**Allow** and tick **Remember**. The permission is stored, so it isn't asked
again.

**X11** needs no extra permission.

**Other Wayland compositors (Sway, Hyprland…)** may need direct access to the
virtual input device. Grant it permanently with a udev rule:

```bash
echo 'KERNEL=="uinput", GROUP="input", MODE="0660", OPTIONS+="static_node=uinput"' \
  | sudo tee /etc/udev/rules.d/60-edgeway-uinput.rules
sudo usermod -aG input "$USER"
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Log out and back in once for the group change to apply.
