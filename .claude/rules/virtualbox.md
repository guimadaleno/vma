---
paths:
  - "src/win/**"
  - "src/core/machine.rs"
  - "src/core/version.rs"
---

# VirtualBox's API

- Every name and number vma calls comes from `VirtualBox.xidl` at the tag of the host's version: `https://raw.githubusercontent.com/VirtualBox/virtualbox/v<version>/src/VBox/Main/idl/VirtualBox.xidl`. Read a method or an enum there before calling it.
- COM spells the names with a capital first letter: `takeScreenShotToArray` is `TakeScreenShotToArray`.
- VBoxManage's own words (state names) are in `src/VBox/Frontends/VBoxManage/` at the same tag.
