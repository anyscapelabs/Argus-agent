# Chat
> **Job:** How sessions, permissions, and approvals work.

## Sessions

Each conversation is a session with its own model, permission mode, and history. Sessions compact automatically when context fills: older messages fold into a summary, the newest stay verbatim.

## Permission modes

| Mode | Behavior |
| --- | --- |
| `ask` | Writes and desktop actions pause for approval first. |
| `never` | The agent acts without asking. |

Read-only work (search, reads, screenshots) never needs approval.

## Approvals

A pending approval shows the exact action with Run / Deny. Denied actions stay denied: the agent explains the failure instead of retrying. Terminal output streams live; long commands cap at 120 seconds.

## Tool results

Every tool call returns `ok` or `err`. On `err` the agent explains what failed and what would fix it, then moves on — it never silently retries the same call, and repeats of one identical action are stopped automatically after two attempts.

## Titles and votes

Sessions auto-title from the first message. Vote messages up/down to mark good answers.
