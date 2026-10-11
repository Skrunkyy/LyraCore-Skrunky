# LyraCore quickstart on Windows

This is the Windows path from a checkout on disk to a stock 1.12.1 client logged into your own
machine. It mirrors [`quickstart.md`](./quickstart.md), which stays authoritative for the realm's
behaviour; this page is only about running it natively on Windows.

**Supported:** Windows 10/11, 64-bit. The gateway, the module build, and the development CLI all run
natively — no WSL, no container, no POSIX shell required. `lyracore.cmd` and `install.ps1` replace
`./lyracore` and `install.sh`.

**Read this first, it decides what you need:**

- **You supply the game client.** LyraCore distributes **no game client, no game assets, and no game
  data** — you must supply your own legally obtained **1.12.1, build 5875** client. This project is
  not affiliated with, endorsed by, or sponsored by Blizzard Entertainment, Inc.
- **This is a local development fixture, not a server you host.** Everything binds to `127.0.0.1`.
  Never expose ports `3000`, `3724` or `8085` to the internet. [`quickstart.md`](./quickstart.md)
  §Security posture explains why.

---

## 1. Prerequisites

| Thing | Version | Notes |
| --- | --- | --- |
| **Rust** | **1.93.0** | Install with [rustup](https://rustup.rs/). `rust-toolchain.toml` makes rustup fetch 1.93.0 (plus the `wasm32-unknown-unknown` target) for any `cargo` command in the checkout. |
| **SpacetimeDB CLI** | **exactly 2.7.1** | `install.ps1` installs it and pins 2.7.1. Manual: `iwr https://windows.spacetimedb.com -useb \| iex`, then `spacetime version install 2.7.1` and `spacetime version use 2.7.1`. |
| **git**, a C toolchain | any current | Cloning and linking the gateway. The MSVC toolchain ships with the Rust installer's default host. |
| **wasm-opt** (Binaryen) | any current, *optional* | `install.ps1` offers a user-local install. Without it `spacetime publish` still works but ships an unoptimised module. |

The repository is pinned to **SpacetimeDB exactly 2.7.1**; `lyracore preflight` refuses any other
version.

### Setup

From the checkout root, in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1
```

It checks the prerequisites, offers to install Rust, SpacetimeDB 2.7.1 and `wasm-opt` (each
user-local, no elevation), then builds the gateway and the vendored Windows CLI. Use `-SkipTools` to
only check and build, or `-Yes` to answer every prompt with yes.

A healthy `doctor` looks like this:

```text
  ✓ project layout   D:\Lyracore\LyraCore-main
  ✓ Rust             1.93.0 (requires 1.93.0)
  ✓ Cargo            cargo 1.93.0
  ✓ SpacetimeDB      2.7.1 (requires 2.7.1)
  ✓ WASM target      wasm32-unknown-unknown installed
  ✓ ports            3000, 3724, 8085 free
```

---

## 2. Start the stack

```powershell
.\lyracore.cmd dev up
```

`lyracore.cmd` is the Windows launcher. It walks up from the current directory to the checkout
(a directory holding `.lyracore-cli-rev`), builds the vendored CLI on first use, and runs it with
your current directory preserved. `lyracore.cmd doctor`, `dev status`, `dev down`, `account create`
and the rest all work from any subdirectory of the checkout.

`dev up` starts SpacetimeDB on `127.0.0.1:3000`, builds the gateway, runs the offline preflight,
publishes the four fixture databases (`lyracore`, `lyracore-kalimdor`, `lyracore-instances`,
`lyracore-realm`), claims the operator identity, and starts the gateway on `127.0.0.1:8085` (logon on
`3724`). It prints its progress and ends with:

```text
✓ dev stack is up.
  spacetime  healthy   (PID 15540, 127.0.0.1:3000)
  gateway    healthy   (PID 20224, 127.0.0.1:8085)
  topology   sharded, 4 database(s)
             lyracore           published, gateway connected
             ...
```

Re-running `dev up` on a healthy stack does nothing. It never clears a database, so characters
survive an up/down cycle.

---

## 3. Create an account

```powershell
"test123" | .\lyracore.cmd account create TEST --password-stdin
```

or run `.\lyracore.cmd account create TEST` for a hidden interactive prompt. The password is read
from stdin or the console, never from an argument; it is capped at 16 bytes and the account name is
uppercased. `TEST` is the seeded fixture account.

---

## 4. Point your client at it

In your 1.12.1 (build 5875) client folder, edit `realmlist.wtf` (client root, or
`Data\<locale>\`) so it reads exactly:

```text
set realmlist "127.0.0.1"
```

Launch the client and log in with the account from §3. The logon server is TCP `3724`; the client
learns the world address (`8085`) from the realm list.

---

## 5. Everyday commands

```powershell
.\lyracore.cmd dev status                # what is running, where, is the database published
.\lyracore.cmd dev logs                  # both components' logs
.\lyracore.cmd dev logs gateway          # just one (spacetime | gateway)
.\lyracore.cmd dev down                  # stop what this CLI started
.\lyracore.cmd dev down; .\lyracore.cmd dev up   # restart (rebuild + republish; nothing wiped)
```

Runtime files live in the git-ignored `.lyracore\` directory: `state.json`, `coordinator-token`
(a credential — keep it), and `logs\spacetime.log` / `logs\gateway.log`.

---

## 6. What is ported, and what is not

**Ported and verified on Windows:** `doctor`, `dev up` / `dev down` / `dev status` / `dev logs`,
`preflight`, `publish`, `account create`, `config`, `character`, `update`.

The Windows CLI is vendored under `tools\lyracore-cli` (a fork of
[`LyraCoreProject/lyracore-cli`](https://github.com/LyraCoreProject/lyracore-cli) at the revision in
`.lyracore-cli-rev`). The only changes are in `src\platform.rs` and a few call sites: process
identity reads the Win32 process creation time and image name instead of `ps`; `dev down` stops the
whole process tree (the Windows `spacetime` shim spawns the node as a child instead of `exec`ing into
it, so the recorded PID is the shim); the password prompt toggles the console echo mode instead of
running `stty`; and the exclusive lock / no-replace rename use the Win32 equivalents. `lyracore.cmd`
builds this CLI.

**Not ported — these still need a POSIX shell or Linux and are refused with a clear error on
Windows:**

- `lyracore dev smoke` and `lyracore import` shell to `bash` scripts (`run-suite.sh`,
  `pull-classic-db.sh`, `import-class-spells.sh`). The fixture is playable without them; the real
  world import is a Linux/macOS task for now.
- `lyracore service` manages a systemd unit and is Linux-only.
- `lyracore packages build` needs [Bun](https://bun.sh/), which is not installed by `install.ps1`.

`lyracore dev smoke`, `lyracore import world` / `import vmaps` and `lyracore service reconcile` print
a sentence saying so instead of failing deep inside a missing subprocess.

The vendored CLI's own `cargo test` is not fully Windows-clean: 18 of 767 tests fail, all in the
not-ported `packages`, `client`, `import` and `dev smoke` areas, whose tests assume POSIX path and
symlink behaviour (Windows `canonicalize` returns a `\\?\`-prefixed path, and symlink creation needs
privileges). The tests covering the ported dev loop pass.

---

## 7. Troubleshooting

### `spacetime` is not recognised

The SpacetimeDB installer adds `%LOCALAPPDATA%\SpacetimeDB` to your **user** `PATH`, which new
shells pick up. In the shell that ran the installer, or one started before it, run:

```powershell
$env:Path = "$env:LOCALAPPDATA\SpacetimeDB;$env:Path"
```

### `lyracore.cmd: not inside a LyraCore checkout`

You are outside every checkout. `cd` into one — its root is the directory holding
`.lyracore-cli-rev` — and run the command again.

### `doctor` says a version is wrong

- **Rust:** the pinned 1.93.0 is fetched automatically by any `cargo` command in the checkout. A
  mismatch usually means `rustc` is not rustup-managed; install rustup and put its `bin` first on
  `PATH`.
- **SpacetimeDB:** `doctor` accepts 2.7.1 *or newer*, but the repo is pinned to exactly 2.7.1:

  ```powershell
  spacetime version install 2.7.1
  spacetime version use 2.7.1
  ```

### `dev down` leaves something on port 3000

Older builds stopped only the recorded shim PID. Update the vendored CLI (`cargo build --manifest-path
tools\lyracore-cli\Cargo.toml`) and re-run `dev down`; if a stale node is still listening, stop it
with `taskkill /IM spacetimedb-standalone.exe /F` and `taskkill /IM spacetimedb-cli.exe /F`.

### A port is held by a process this CLI did not start

`dev status` says `external`. A foreign SpacetimeDB node is reused; a foreign **gateway** is not
adopted. Stop the offender with `taskkill /IM lyracore-gateway.exe /F`, or run `lyracore dev down`
from the checkout that started it.

### The client says "Unable to connect"

Wrong `realmlist.wtf` (check the client root **and** `Data\<locale>\`); the stack is not up
(`lyracore.cmd dev status`); port `3724` is blocked by Windows Firewall; or the client is not build
5875.
