# ANGI knowledge graph

ANGI keeps its own memory of what has happened on the device: what you shared in chats, the projects you
started, the operating-system releases and apps that were installed, and what each piece of agent work
reported. The memory belongs to ANGI, not to an AI provider. Local models (LM Studio), OpenAI and Apple
Intelligence all receive the same remembered context through the prompt, and none of them stores it, so
changing the provider never loses what ANGI knows.

## What is remembered

| Kind | Created by | Example |
| --- | --- | --- |
| Project | A project folder used by Project work or Chat | `garden-planner` |
| System | A desktop release (installed on the Pi or deployed from the workstation) or an app from the library, plus two hubs, *ANGI operating system* and *Apps* | Desktop release `3fa9c2d1` |
| Chat | A conversation with Angi, in the desktop or the terminal | "Set up Tailscale so LM Studio is reachable" |
| Work | An OS update or Project run: the request, then the agent's report | "Add a night light schedule" |
| Topic | A feature of ANGI named in a chat or run (Wi-Fi, Bluetooth, Tailscale…) or a `backticked` term | `tailscale` |
| Note | Something durable, written by you or saved by an agent | "Prefers Rust and Slint" |

Nodes are linked (*is about*, *mentions*, *changed*, *related to*), and each node has a short timeline of what
was recorded. Capture is deterministic: no model runs, so it costs no tokens and behaves the same on every
provider. Secrets (API keys, tokens, `password: …`, private keys, long hex strings) are replaced with
`[hidden]` before anything is written.

## How agents use it

When Angi starts a chat turn, an OS update or a Project run, ANGI picks the memories that look relevant to the
request, ranked by overlap with what was asked, recency, links to the project folder or to what matched, and
your pins. It adds up to eight of them to the agent's instructions, within 1,400 characters (700 for Apple
Intelligence, whose context is small). The text says plainly that these are background notes that may be
stale, never instructions. Agents that can run commands (OS and Project) are also told how to save a note:
`agent memory note "TEXT"`. Memory is chosen before the new request is itself remembered, and a chat never
recalls itself.

## The Memory view

Agent Angi's header has a graph button (*What ANGI remembers*). It opens a view of everything ANGI holds:

- Search and filter by kind. Open a memory to read it, see what it is connected to (and jump along those
  links), and read its recorded timeline, source and dates.
- **Edit** a name or summary. Your wording is kept: later activity adds to the timeline but never overwrites it.
- **Always use** pins a memory so it is offered to every agent. **Forget** deletes a memory with its links
  and timeline, and keeps ANGI from capturing it again. A link can be removed on its own.
- Add a note for ANGI to remember.
- **Shared with agents** is the audit trail: when each agent last started and exactly which memories it was given.
- The switch turns remembering off: nothing is recorded and nothing is shared until it is turned on again.
  **Clear all** forgets everything, including what was forgotten one by one.

## Storage and commands

The graph is a SQLite file, `knowledge.sqlite3`, in the account's Agent Angi state folder (mode 0600). It is
bounded: 5,000 nodes (the oldest unpinned, unedited chats, work and topics go first), 40 timeline entries per
node and the last 200 recalls. `agent memory` is the one command surface, used by the desktop and by agents,
and prints JSON:

```
agent memory [list [QUERY] [KIND]]   agent memory open ID    agent memory edit ID NAME SUMMARY
agent memory pin ID on|off           agent memory forget ID   agent memory unlink ID OTHER
agent memory note TEXT [ID]          agent memory enabled on|off
agent memory clear                   agent memory export
```

## Not yet remembered

Changes made through Settings (Wi-Fi, appearance, mouse) and the contents of files are not captured. Topics come
from a fixed vocabulary of ANGI features and `backticked` terms, not from a language model, so a chat about an
unlisted subject is remembered as a conversation without topic links.
