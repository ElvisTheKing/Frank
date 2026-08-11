# Frank UI Control plugin

This repo-local Codex plugin drives Frank's real egui application in the isolated `ui-test` Docker service. It contains the MCP launcher and the repeatable QA workflow; no user-specific path is stored in the package.

Codex discovers it through `.agents/plugins/marketplace.json` when this repository is opened. Install **Frank UI Control** from the **Frank Development** marketplace, restart Codex after plugin updates, and begin a new task so the bundled skill and MCP server are loaded.

If Codex is launched outside the checkout, set `FRANK_REPOSITORY` to the checkout's absolute path. The launcher otherwise finds the repository by walking upward from the task working directory.

The launcher can also be tested directly from the repository root:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File plugins/frank-ui-control/scripts/start-frank-egui-mcp.ps1
```

It intentionally stays attached to the stdio MCP server until its caller disconnects.
