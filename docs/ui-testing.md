# Agent-driven UI testing

Frank has two complementary UI-test surfaces:

1. `egui_kittest` exercises the semantic AccessKit tree in-process. These tests locate controls by accessible label, inject actions, and assert the resulting `UiOutput` without opening a native window.
2. The `ui-test` Compose service runs the complete eframe/WGPU application in an isolated Linux container. Xvfb provides the virtual display, Mesa Lavapipe provides a deterministic software Vulkan adapter, and egui's inspection protocol exposes the live widget tree, input injection, window resizing, and screenshots on TCP port 5719.

The live service publishes inspection only on host loopback. Do not change the host binding to a LAN address: the inspection protocol intentionally has no authentication and grants full control of the application.

## Start the live application

```powershell
docker compose up --build -d ui-test
docker compose ps ui-test
docker compose logs -f ui-test
```

The service generates two deterministic JPEG fixtures under `/tmp/frank-fixtures`, starts Frank with those paths, and writes renderer/environment diagnostics to `artifacts/ui-tests/environment.txt`.

Use a light-theme pass when needed:

```powershell
$env:FRANK_TEST_THEME = "light"
docker compose up --build -d ui-test
```

Remove the environment override to return to the default dark-theme pass.

## Install the repo-local Codex plugin

Everything needed by Codex lives in this repository:

- `.agents/plugins/marketplace.json` advertises the **Frank Development** repo marketplace;
- `plugins/frank-ui-control` contains the plugin manifest, QA skill, and portable MCP launcher;
- the launcher discovers the checkout from the task working directory or `FRANK_REPOSITORY`, then starts the Compose service itself.

On a fresh PC, install Git, Docker Desktop (using Linux containers), Codex, and Node.js 20 or newer. Clone the repository, open that checkout in Codex, restart Codex so the repo marketplace is discovered, and install **Frank UI Control** from **Frank Development**. Start a new task after installing or updating it so its MCP server and skill are loaded.

No global plugin copy or hard-coded checkout path is required. If Codex is intentionally started outside the checkout, set `FRANK_REPOSITORY` to its absolute path before starting Codex.

To verify the exact `.mcp.json` entry point shipped in the plugin rather than connecting directly through Compose:

```powershell
$env:FRANK_MCP_CONFIG = "plugins/frank-ui-control/.mcp.json"
$env:FRANK_SMOKE_NAME = "plugin-diagnostics"
$env:FRANK_SMOKE_FLOW = "diagnostics-after-rotation"
node scripts/frank-ui-smoke.mjs
```

## Drive Frank through egui MCP

The repo-local Codex plugin starts the Compose service and exposes `egui_mcp` over stdio. Its connection lifecycle is:

1. Call `attach` with `127.0.0.1:5719`. The MCP server runs inside the same container as Frank.
2. Use `query_tree` or `get_node` to locate controls by their accessible labels.
3. Use `click`, `drag`, `type_text`, `press_key`, `scroll`, or `batch` to exercise a flow.
4. Wait for the label `Frank automation status: ready` before asserting a completed load or background operation.
5. Use `screenshot` for visual review and save the returned image with the action log under `artifacts/ui-tests`.

The rotation smoke flow is:

- wait for `Frank automation status: ready`;
- click `Pane 2 image viewport: target.jpg`;
- click `Align`;
- click `Rotate right 90°`;
- verify the status text reports pane 2 at `+90.0°`;
- capture the full window;
- confirm both the egui controls and custom WGPU image panes are present.

Run that complete flow from the repository root with Node.js 20 or newer:

```powershell
$env:FRANK_SMOKE_NAME = "dark"
node scripts/frank-ui-smoke.mjs
```

`FRANK_SMOKE_NAME` prefixes the screenshots, widget trees, and action transcript so dark and light passes can be retained together.

## Testability contract

- Interactive controls must have stable, unique accessible labels. Custom-painted controls must call `Response::widget_info`.
- Repeated pane controls include the pane number in their accessible label.
- Asynchronous operations expose `busy`, `ready`, or `error` through the automation-status label; tests do not use fixed sleeps as completion checks.
- Test mode fixes the theme, animation time, fixture paths, configuration directory, cache directory, window size, and software renderer.
- Normal workflow tests load images through startup arguments. Native file-picker behavior is outside this Linux-container suite.
- Every failed live pass should retain its screenshot, widget-tree extract, action transcript, Docker logs, and `environment.txt`.

## Run the deterministic suite

```powershell
docker compose run --rm dev cargo test --workspace
docker compose run --rm dev cargo clippy --workspace --all-targets -- -D warnings
```

The `ui-egui` interaction test opens the Align menu through AccessKit, verifies the fine slider and custom pane semantics are discoverable, activates the 90-degree control, and asserts the emitted registration request.

## Stop and reset

```powershell
docker compose stop ui-test
docker compose rm -f ui-test
```

All application configuration and generated fixtures live inside the disposable container. Only files under `artifacts/ui-tests` persist on the host, and that directory is ignored by Git.
