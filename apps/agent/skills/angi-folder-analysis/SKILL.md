---
name: angi-folder-analysis
description: Analyze a folder the user attached to a message. Use when an attachment named folder-*.md (a folder summary) is present, or when the user asks what a folder contains, how a project is organized, or what to do with it.
---

# Analyzing an attached folder

The desktop attaches a folder as a Markdown summary named `folder-NAME.md`: the real path, file and
folder counts, file types, the largest files, an indented layout (hidden items, build folders and
links left out) and the start of key files such as the README. Read that summary first. It is a
snapshot, so say so if you rely on it.

## Work out

1. **What it is.** Purpose, kind of project (code, documents, media, backups, mixed) and the main
   language or tooling, from the README, manifest files and the file-type counts.
2. **How it is organized.** The main folders and what each holds; entry points; where tests,
   docs and configuration live; anything that looks misplaced, duplicated or abandoned.
3. **What stands out.** Very large files, odd file types, missing basics (README, tests, license,
   `.gitignore`), signs of generated or vendored content, and anything that looks sensitive
   (keys, credentials, personal data). Name such files; never print their contents.
4. **Size and health.** Where the space goes and what could safely be trimmed.
5. **What to do next.** Two to five concrete, ordered suggestions that fit what the user asked.

## By mode

- **OS and Project agents:** the summary names the real path. Read further with your file tools
  (list folders, open key files, search) before concluding, and cite the files you read. Do not
  change or delete anything in the folder unless the user asked for that.
- **Chat:** work from the summary alone. Say plainly what you could not see, and offer to look
  deeper once the user picks the folder with the folder button beside the message box, because
  Chat reads only the folder it was given.

## Answer shape

Open with a one- or two-sentence verdict. Then short sections: **What it is**, **Layout**,
**Worth a look**, **Next steps**. Keep it scannable; prefer a few precise points over a long tour.
