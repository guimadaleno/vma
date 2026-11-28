---
name: vma
description: "Operate a VirtualBox machine with vma: screen, mouse, keyboard, scripts, files. Use for any task on a VM."
argument-hint: <machine>
---

`vma help` first: commands and options come from it, never memory. Not installed: install from guimadaleno.com/vma only with the user's OK.

- The machine given, else the only one `vma list` shows; several: ask which.
- Before acting: running, and its agent answering where it has one.
- Look at a picture before acting, and after.
- The agent over pixels: run, wait and click by name where it can.
- Done is a picture or a script's output, not an exit code.
- Create or destroy a machine only when asked.
- VirtualBox only through `vma`; never a snapshot: one has frozen a machine.
- Install in the machine whatever the task needs, no OK asked: it holds nothing to lose. Nothing private goes in.
- Stop the machine you started or were handed, unless a session waits for it: then hand it over running, what you opened closed. Frozen after a restart: a forced stop, then a start.
