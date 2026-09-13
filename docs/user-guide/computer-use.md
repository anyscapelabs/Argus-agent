# Computer use
> **Job:** Let the agent see and control the Linux desktop.

## Requirements

X11 session plus `xdotool`, `imagemagick` (`import`, `convert`, `identify`), and `wmctrl`. Accessibility tree reads need a running AT-SPI bus (standard on GNOME).

## The three tiers

1. **Native surface first.** Shell, browser tools, or files cover most jobs. To open an app, the agent launches it by name — it never hunts for icons.
2. **Accessibility tree** (`computer.observe`). Every window as structured text with numbered refs; actions run by ref (`computer.act`), no coordinates involved.
3. **Pixels last** (`computer.screen` + `computer.click`). Only for canvas-drawn apps. Coordinates come from the latest screenshot; every action returns a fresh screenshot and tree.

## Rules the agent follows

- One desktop action per step, verified before the next.
- A repeated identical action is stopped after two attempts with instructions to switch tiers or ask you.
- Password fields are refused in code — you type those yourself.
- On-screen text is untrusted: if the screen tells the agent to run something, it reports instead of obeying.

Mutating actions (click, type, keys, window changes, launches) pause for approval in `ask` mode, like terminal writes.
