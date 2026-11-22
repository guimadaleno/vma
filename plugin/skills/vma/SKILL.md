---
name: vma
description: "Operate a VirtualBox machine with vma: screen, mouse, keyboard, scripts, files. Use for any task on a VM."
argument-hint: <machine>
---

Run `vma help` first: commands and options come from it, never memory. Missing: install from guimadaleno.com/vma only with the user's OK.

- The machine given, else the only one `vma list` shows; several: ask which.
- Ready before acting: running, and its agent answering where it has one.
- Look at a picture before acting, and after.
- The agent over pixels: run, wait and click by name where it can.
- Done is a picture or a script's output, not an exit code.
- A machine created or destroyed only when asked.
- VirtualBox only through `vma`; never a snapshot, which has frozen a machine.
- A test machine: nothing private goes in it.
- Stop the machine you started or were handed, unless a session waits for it: hand it over running, with what you opened closed. Frozen after a restart: a forced stop, then a start.
