# ANGI agent workshop

Run **`agent angi`** from a normal ANGI Terminal or the TUI's shell. Choose
**N**, describe what to build, then press Enter. The workshop opens Herdr with
three OpenCode agents: orchestrator on the left, builder and tester on the
right. Use **Ctrl+B, then Q** to detach while they continue working. Run
`agent angi` and choose **R** to resume. A new build creates a separate named
session; old sessions remain available in Herdr rather than being killed.

Closing the terminal tab also detaches Herdr, and the agents keep running (and keep their memory).
While any are running, Agent Angi's page shows an **Open Herdr** banner with the agent count. It
focuses the tab already showing Herdr or opens a new one, running **`agent attach`**, which opens
the newest swarm whose Herdr server is still up.

Herdr shows one session at a time, so the banner's agent count can be larger than what Herdr shows:
every earlier build still has its own Herdr server, three agents and a mail-delivery worker that
polls the server every two seconds. **Stop** (it asks first) ends every Agent Angi session of the
account at once: servers, agents and workers. The files the agents wrote stay; only the running
sessions end. It does not touch OS-update runs or other Herdr sessions.

Choose **C** to open OpenCode, then run `/connect` to authenticate a model
provider. The agents use OpenCode's configured/default model and the current
account's provider credentials. Credentials are not copied from Codex or from
another account. Three agents can generate concurrent model usage.
The built-in Big Pickle model was also verified on the development Pi without
saved credentials; availability of that service is controlled by OpenCode.

The orchestrator receives the build brief and coordinates work. Builder edits
the shared checkout; tester independently validates changes. Their role
profiles tell them how to assign work and report results without spawning
additional agents. `agent tell builder 'message'` queues durable peer mail;
use orchestrator or tester for the other recipients. A small background
dispatcher submits queued messages when the receiving agent is idle/done.
Delivery means submission to OpenCode; model/provider failures still need
attention in that pane. `agent inbox` reads messages inside a swarm pane;
`agent status` shows the last swarm's live agents.

## Native development and administration

Source is `/var/lib/agx-data/angi-dev`, initialized as a local Git repository
from the workstation's current source, including uncommitted files. It is an
independent checkout; future edits are not automatically synchronized with the
workstation or pushed to GitHub. The SDK and caches are also on NVMe.

- `angi-build cli`: release CLI build on the Pi.
- `angi-build gui`: release LinuxKMS GUI build on the Pi. It runs at the lowest priority with one core left
  free, so the desktop stays responsive. A small UI change rebuilds in about a minute. When the dependency
  manifests or compiler changed since the last good build there (for example after a workstation release added
  a feature), it compiles the dependencies again (about 14 minutes) and says so first. `make deploy-gui` starts
  one such build in the background after installing, so an agent's first change is already incremental; its log
  is `/var/lib/agx-data/agx-opencode-users/1000/angi-warm.log`.
- `angi-build test`: root workspace tests.
- `angi-build gui-test`: standalone GUI tests with LinuxKMS enabled.
- `source "$(angi-build env)"`: native build environment for custom commands.

The SDK combines checksum-pinned Debian ARM64 GCC 12/binutils, make, pkgconf
and Python 3.11 packages with the development image's own Buildroot sysroot,
plus Rust 1.97.1. Debian packages are extracted privately; no package manager
or Debian libc is installed into the live system. Compiler and Python wrappers
isolate their dependency libraries. Rust formatting and Clippy components are
included by the provisioning script. SDK package versions, hashes and upstream
copyright files are retained. Full Buildroot image/kernel rebuilds still need
a native-host toolchain configuration; the repository's image configuration
currently targets the workstation's cross toolchain.

The installed `angi-development` skill documents ANGI's source boundaries,
native builds, services, app installation and rollback. It is loaded from the
account's NVMe OpenCode configuration. Profiles also explicitly instruct each
agent to read it before work.

`angi-root COMMAND ARG...` grants full root execution to `agx-admin` members,
as requested for this workshop. The helper is a root-owned `4750` executable
on the system partition because NVMe is mounted `nosuid`. It validates group
membership, clears the inherited environment, and logs caller UID and program
name, excluding arguments. OpenCode and ordinary builds run as the user.
For shell redirection, use `angi-root /bin/sh -c '...'` explicitly.

## Provisioning

Install OpenCode and its terminal runtime first. With the matching Buildroot
sysroot present under `output/staging`, run:

```sh
AGX_SSH_HOST=root@10.78.0.2 AGX_SSH_HOST_KEY_ALIAS=10.77.0.2 make agent-sdk
AGX_SSH_HOST=root@10.78.0.2 AGX_SSH_HOST_KEY_ALIAS=10.77.0.2 make deploy-agent
```

`AGX_DEV_USER` defaults to `dana`. Provisioning preserves an existing source
workspace. The runtime is Herdr 0.9.5, verified against its published ARM64
release digest; deployments retain versioned releases on NVMe. The image
package `agx-agent` includes the launcher, Python modules, Herdr and root
helper; the SDK and source are separately provisioned development data.

OpenCode/Herdr state, plugin caches, delivery logs, prompts and mail are private
to each account under `/var/lib/agx-data/agx-opencode-users/<uid>`. Herdr's
official integration reports agent state and conversation IDs; its installer
is given a temporary private HOME because this version assumes a fixed
OpenCode configuration path. Actual agent HOME remains the normal user home.

Run `make agent-test` for model-free messaging/isolation tests. Native builds,
privilege checks, Herdr layout, attach/detach, and live model responses require
the Pi; test those separately and record provider failures accurately.

Verified on the development Pi on November 17, 2026: native workspace tests,
all 105 standalone GUI tests, a release CLI build, C/C++ compilation, allowed
and denied administrator execution, and five messaging/isolation tests. The
three live Big Pickle agents exchanged orchestrator-to-peer messages and a
builder/tester round trip. Detach/resume preserved the agents; stopping and
restarting their Herdr server restored all three native conversation IDs.
These checks did not rebuild a complete OS image or require an HDMI monitor.

Upstream: [Herdr 0.9.5](https://github.com/herdrdev/herdr/releases/tag/v0.9.5)
and the [Herdr CLI reference](https://herdr.dev/tui/docs/cli-reference/).


## What Agent Angi does: OS, Project or Chat

A three-way switch above the prompt chooses the scope of a run. Hover over an option for details.

* **OS** (the default) works on the operating system.
  Install the native SDK and source in `/var/lib/agx-data/angi-dev` before starting OS work.
  OS builds use `angi-build`, and desktop releases are installed through `angi-update`.
* **Project** builds the user's own software. A
  **Select folder…** button appears for the project folder. **Build** stays disabled until one is
  chosen. The run starts in that folder and the agent instructions limit work to that folder.

* **Chat** is a conversation with Angi in the panel. Your messages sit on the right, with
  selectable replies on the left and a status while Angi works. Starter prompts fill the
  composer for review. Press **Enter** to send or **Shift+Enter** for a new line.
  **New chat** clears the conversation. Chat uses one agent: each message runs `agent chat-turn [--new | --session ID] [--folder PATH] MESSAGE`
  (`apps/agent/chat.py`), which calls `opencode run --agent angi-chat --format json` and prints one
  JSON line with `text` or `error`, plus `session_id` when available. Chat can list directories, read UTF-8
  text, create folders, and save text in the user's home folder. For example, ask Angi
  to create `Test` under home, then ask her to save `index.html` inside it.
  **Change folder…** chooses a different workspace, and **Use Home** returns to the default.
  The file tools skip hidden files and symbolic links. Replacing an existing file
  requires an explicit overwrite request. The conversation works in a private `chat` folder
  in the swarm state directory; the first message of a conversation passes `--new`, later ones
  continue that session by ID. `agent angi --prompt BRIEF --chat` still runs the same agent in a terminal.
  With Apple Intelligence
  active, Chat starts the local request guard and uses the server's first model if no model
  was chosen. For other providers, OpenCode supplies its default when no model was chosen.
  The AI button in the upper-right corner opens provider setup from any mode.

When Chat saves HTML, ANGI Web opens beside Agent Angi on the right. Ask Angi to
show a project in Chat to reopen its preview. You can give a folder or HTML path,
refer to the selected Project folder, or ask to show the last preview.

For OS and Project work, `agent preview PATH` shows an HTML file or the entry page
of a static output folder. Pass a loopback URL to show a development server the
agent has started. If the agent has yet to open a preview when it saves its work
report, ANGI opens a changed HTML page from the report and prefers an index page.

The button at the bottom of the OS and Project composer switches between **Swarm**
and **Single agent**. Swarm is the default and starts the Herdr orchestrator, builder
and tester. Single agent starts one OpenCode session in the selected scope. Choose
its model in **AI providers → Models → Single agent (OS and Project)**. This choice
applies to new single-agent runs and has its own setting alongside Chat and the
three swarm roles. Without a selection, Apple Intelligence uses the local server's
first model. Other providers use the OpenCode default.

The Angi logo opens a history drawer on the left. It lists past conversations and build briefs
by subject, with **OS update**, **Project**, and **Chat** badges. For sessions without a subject,
the drawer uses a short title based on the first prompt. Selecting a Chat entry reopens its
transcript and file workspace. Selecting an OS or Project entry opens the original request, the recorded outcome, and
links to relevant folders in ANGI Files. Right-click an entry to rename it or remove it from the drawer.
These changes stay in Agent Angi history and preserve the original chat, run and project files.
The single agent and swarm orchestrator save results with `agent report SUMMARY [PATH ...]`.
Older entries show the last recorded reply when the run can be matched to its OpenCode session.

`agent angi` resumes the most recent Chat, Single agent or Swarm run.
Use `agent angi --prompt BRIEF --single` to start a single OS agent from the
terminal, or add `--project DIR` before `--single` to work in a project.

The scope is the folder the agents start in plus a scope block appended to each prompt
(`apps/agent/scope.py`); the swarm role profiles themselves are scope-neutral. It steers the agents but is
**not a sandbox**: they run as the desktop account with its permissions. Runs record their mode in
`run.json`, so resuming a run restores its scope, and runs from before project mode resume as OS runs.

The agent runtime is the only judge of a folder (`agent project PATH` prints one JSON line; launch
checks again). It accepts a folder you own and can change inside your home or on the data volume
(including `users/<you>`), and refuses: ANGI's own source (use OS mode), system and credential
folders under the data volume, your home or the data volume themselves, hidden or settings
folders, anything outside those locations, and links that lead out of them.

### Choosing the folder with ANGI Files

The project folder enters through two hooks on the `AgentScope` global in `ui/agent-scope.slint`:

* `choose-project()`: the user asked to pick a folder. `agent_project.rs` opens ANGI Files'
  folder picker, a modal overlay (`ui/files-picker.slint`) over the desktop, starting in the
  current project or your home folder.
* `use-project(path)`: a folder was chosen. The picker's "Choose" button calls it with the selected
  folder, or the folder being viewed when nothing is selected. The runtime validates it.

An accepted folder closes the picker and appears in the prompt. A rejected one keeps the picker
open and shows the runtime's reason beside the buttons, so the user can pick another without
starting over. The picker is the full file browser limited to folders (new folder, rename, trash,
search and the places sidebar all work), so the rules above stay in `scope.py` and the picker never
duplicates them. Remembering the last project is not stored today (the choice resets with the
desktop).

## OS updates: `angi-update` and the revert guard

The single-agent `angi-system-changes` skill selects the smallest workflow.
Use existing preferences for settings, the agent runtime for skills and Python
tools, and the desktop updater for GUI code. Base system and boot changes use
the board's Buildroot workflow.

`angi-update gui` builds the desktop from the checkout (incremental), installs it as a new
versioned release and switches `/usr/bin/agx-gui` atomically, keeping the previous release.
`angi-update status` shows the installed release and whether an install is allowed;
`angi-update rollback` restores the previous release.

`angi-update refresh` loads the installed release into the running desktop without the Settings
button. The desktop treats `SIGUSR1` from its own account as a refresh request: it saves settings,
closes its terminals and browser sessions and execs `/usr/bin/agx-gui` again, exactly as the button
does. Because that closes the terminal the agent runs in, the command returns at once and a detached
watchdog sends the signal three seconds later; the agent writes its report first and the user resumes
the run afterwards. The watchdog then makes sure the new release is running and stays up for twenty
seconds, and if it does not, restores the previous release so the supervisor starts a working
desktop. The outcome is in `angi-update status` (`running`, `last_refresh`). The command refuses
when the running desktop was built before this handler existed, because `SIGUSR1` would terminate
it; one manual **Overview → Refresh desktop…** is needed after upgrading to the first release that
has it. Do not emulate a refresh with `kill` or `killall`: that exits the desktop without restarting
it.

The updater exists to make one failure impossible: building from an older checkout and replacing a
newer desktop. Every installed release is recorded in `/var/lib/agx-data/agx-gui/ledger.json` with a
git hash of the exact source it was built from, and the source is kept in the checkout's history as
`refs/angi/<release>`. The checkout carries `.git/angi-base`, naming the release it is based on.
`angi-update gui` refuses unless that names the release installed now, and it refuses when the source
changes during the build. A release installed any other way therefore blocks stale installs until
the checkout contains its source.

`make deploy-gui` from the workstation records the release it installs and syncs the workstation's
source into the checkout, so device-side updates start from what is installed. The checkout's own
unsaved edits are first kept as `refs/angi/pi-edits-*`, so a sync never destroys work. If the
device's agent runtime is not installed yet, deploy it first (`make deploy-agent`) and deploy the GUI
again. Skills tell the agent never to link, copy or move releases, to stop when `angi-update`
refuses, and never to reset, stash or clean the checkout.

`make deploy-gui-only BOARD=rpi5` builds just the desktop on the workstation
using the board's existing Buildroot toolchain and GUI features. It sends the
binary and its source snapshot, verifies them on the device and uses the same
release ledger and rollback. It requires the current agent runtime, a recorded
desktop release, and compatible runtime libraries already installed. Finish
with `angi-update refresh` in the authenticated session. Use the full deployment
when changing the desktop's runtime, startup binaries, or boot configuration.

The target also selects the x86-64 toolchain for `BOARD=x86-desktop` (AMD or
Intel). That path requires an existing desktop-enabled board build and a
provisioned device with the agent runtime. The checked-in x86 desktop image
configuration does not yet enable the desktop, and the Buildroot agent
package currently targets ARM64.

## Provider setup on workspace 2

Open the **AI providers** drawer from the upper-right corner and select a provider. OpenAI uses
OpenCode’s browser OAuth flow for a ChatGPT subscription. Finish sign-in in the
browser, then return to Agent Angi; the panel checks stored authentication and
shows the provider’s coding models. Closing the browser does not itself finish
sign-in. Choose each role’s model independently; preferences are saved privately
and applied to new builds, while existing sessions retain their configuration.

Apple Intelligence uses [Local LLM Server](https://apps.apple.com/us/app/local-llm-server/id6757007308)
on an Apple Intelligence device. Enable Apple Intelligence, wait for its model
to download, start the app’s OpenAI-compatible server in LAN mode, and keep it
running on the same trusted network as ANGI. Enter the app’s device address and
port, rather than localhost; ANGI queries `/v1/models` before saving it. The
current setup supports servers without optional Bearer-token authentication.
The app offers optional API keys; use a trusted network for this setup.

The Apple model has a small window: 8,192 tokens of context, and an earlier instance of the server
also refused any request body over 16 KB with `Payload Too Large`. OpenCode's own first request is
about 25 KB, so only for this service ANGI does two things (`apps/agent/applefit.py`). Agents get
fewer tools (web fetch, todo, glob, grep and the skill tool are off; skills are read as files), and
OpenCode talks to a small loopback guard on `127.0.0.1:18081` instead of the server directly. The
guard starts with the run and exits after an hour without requests. It trims any request over
about 15 KB in an order that loses the least (long tool output, then the skill list, then tool
descriptions, then the oldest steps, always keeping the task and the latest step whole) and caps the
reply at 2,048 tokens. Bytes only approximate tokens, so when the server still refuses a request
the guard reads how far over it was, retries smaller (up to three times) and keeps the tighter
budget. The saved address is read on every request. OpenAI and LM Studio runs do not use the guard.
A tool that returns a very long result therefore reaches the model shortened, with a note saying
how much was left out. Check it with `python3 scripts/test-agent-applefit.py`.

The drawer also lists LM Studio, a model served from your own computer and reached privately over
Tailscale. See [Tailscale for LM Studio](tailscale.md) for the setup, the userspace-networking
design and the safety settings. Claude subscription sign-in works only in the unmodified Claude Code
application, so ANGI no longer offers a Claude option. See [Anthropic’s authentication guidance](https://code.claude.com/docs/en/legal-and-compliance).

Agent Angi uses **one provider at a time**. The drawer identifies the active provider and its
shared OS and Chat model. Select a connected provider, then choose **Use** to switch new
requests to it. The active provider's settings list the project role models. Model choices
are saved per provider and restored when you switch back.

Provider setup can be checked without real accounts using
`python3 scripts/test-agent-providers.py` and
`python3 scripts/test-agent-models.py`.
