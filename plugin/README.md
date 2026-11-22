# vma for Claude Code

A skill that lets an agent operate a VirtualBox machine through vma: its
screen, mouse and keyboard, and scripts and files inside it. The agent reads
`vma help` before any command, looks at the screen before and after each
act, and prefers vma's agent inside the machine to clicking by pixels.

The skill runs only the `vma` command, which must already be installed on a
Windows host with VirtualBox. When it is missing, the agent points you to
vma's page, guimadaleno.com/vma, and installs it only with your OK. The skill
sends nothing anywhere and reads no credential.

```
/plugin install vma --marketplace guimadaleno/vma
```

MIT licensed. vma itself, its source and its install instructions:
https://github.com/guimadaleno/vma
