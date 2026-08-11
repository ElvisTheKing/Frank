---
name: frank-ui-qa
description: Exercise and visually inspect Frank's real egui desktop interface in its Docker-isolated Xvfb and software-Vulkan environment. Use for semantic UI actions, rotation/alignment regression checks, widget-tree inspection, screenshots, or validating UI changes before a Windows build.
---

# Frank UI QA

Use the bundled `frank-egui` MCP server for live UI work. It discovers the Frank checkout from the current working directory, starts the `ui-test` Compose service, waits for it to become healthy, and runs `egui-mcp` inside the container.

## Prerequisites

- Work from a cloned Frank repository.
- Docker Desktop must be running with Linux containers enabled.
- Set `FRANK_REPOSITORY` to the checkout's absolute path only when Codex is not opened in that checkout.
- Node.js 20 or newer is required only for the repository smoke script.

## Live semantic workflow

1. Call `attach` with host `127.0.0.1`, port `5719`, and a 30-second timeout.
2. Call `wait_for` with `content_contains: "Frank automation status: ready"` before assertions.
3. Locate controls using stable accessible labels through `query_tree`; do not rely on screen coordinates when a semantic target exists.
4. Exercise the interface with `click`, `drag`, `press_key`, `type_text`, `scroll`, or `batch`.
5. Use `screenshot` after meaningful state changes and retain the action transcript and widget trees under `artifacts/ui-tests`.
6. Call `disconnect` when finished.

The inspection port is intentionally bound only to host loopback and has no authentication. Never expose it to a LAN interface.

## Rotation regression

1. Wait for the automation-ready label.
2. Click the target pane using the label containing `Pane 2 image viewport`.
3. Open `Align`.
4. Click `Rotate right 90°`.
5. Wait for content containing `rotation adjusted to +90.0°`.
6. Capture the full window and verify that both custom WGPU image panes and egui controls are visible.

## Alignment diagnostics regression

1. Select pane 2 and open `Align`.
2. Click `Auto align active to reference`, close the menu, and wait for the automation-ready label.
3. Reopen `Align` and wait for content containing `Last auto align`.
4. Verify `Show match diagnostics` is enabled, toggle it, and confirm the diagnostic overlay appears.
5. Adjust the control labeled `Fine rotation`, then toggle diagnostics off and on again to ensure the overlay remains available after manual rotation.

For the deterministic scripted version, run from the repository root:

```powershell
$env:FRANK_SMOKE_NAME = "diagnostics-after-rotation"
$env:FRANK_SMOKE_FLOW = "diagnostics-after-rotation"
node scripts/frank-ui-smoke.mjs
```

See `docs/ui-testing.md` in the repository for the testability contract, artifact locations, reset commands, and manual launcher fallback.
