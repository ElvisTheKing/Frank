import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const repository = process.cwd();
const artifactDirectory = path.join(repository, "artifacts", "ui-tests");
const runName = process.env.FRANK_SMOKE_NAME ?? "rotation-smoke";
const flow = process.env.FRANK_SMOKE_FLOW ?? "rotation";
const pluginLauncher = process.env.FRANK_MCP_LAUNCHER;
const pluginConfig = process.env.FRANK_MCP_CONFIG;
fs.mkdirSync(artifactDirectory, { recursive: true });

let serverCommand = "docker";
let serverArguments = ["compose", "exec", "-T", "ui-test", "egui-mcp"];
if (pluginConfig) {
  const configPath = path.resolve(repository, pluginConfig);
  const parsedConfig = JSON.parse(fs.readFileSync(configPath, "utf8"));
  const servers = parsedConfig.mcp_servers ?? parsedConfig.mcpServers ?? parsedConfig;
  const serverConfig = servers["frank-egui"];
  if (!serverConfig?.command || !Array.isArray(serverConfig.args)) {
    throw new Error(`frank-egui server is missing from ${configPath}`);
  }
  serverCommand = serverConfig.command;
  serverArguments = serverConfig.args;
} else if (pluginLauncher) {
  serverCommand = "powershell.exe";
  serverArguments = [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      path.resolve(repository, pluginLauncher),
      "-Repository",
      repository,
    ];
}
const server = spawn(
  serverCommand,
  serverArguments,
  {
    cwd: repository,
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true,
  },
);

let nextId = 1;
let stdoutBuffer = "";
let stderr = "";
const pending = new Map();
const actions = [];

server.stdout.setEncoding("utf8");
server.stderr.setEncoding("utf8");
server.stderr.on("data", (chunk) => {
  stderr += chunk;
});
server.stdout.on("data", (chunk) => {
  stdoutBuffer += chunk;
  for (;;) {
    const newline = stdoutBuffer.indexOf("\n");
    if (newline < 0) break;
    const line = stdoutBuffer.slice(0, newline).trim();
    stdoutBuffer = stdoutBuffer.slice(newline + 1);
    if (!line) continue;
    const message = JSON.parse(line);
    if (message.id === undefined) continue;
    const request = pending.get(message.id);
    if (!request) continue;
    pending.delete(message.id);
    clearTimeout(request.timer);
    if (message.error) request.reject(new Error(JSON.stringify(message.error)));
    else request.resolve(message.result);
  }
});
server.on("exit", (code) => {
  for (const request of pending.values()) {
    clearTimeout(request.timer);
    request.reject(new Error(`egui-mcp exited with code ${code}: ${stderr}`));
  }
  pending.clear();
});

function request(method, params = {}) {
  const id = nextId++;
  const payload = { jsonrpc: "2.0", id, method, params };
  server.stdin.write(`${JSON.stringify(payload)}\n`);
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`Timed out waiting for ${method}`));
    }, 60_000);
    pending.set(id, { resolve, reject, timer });
  });
}

function notify(method, params = {}) {
  server.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method, params })}\n`);
}

function compactResult(result) {
  return {
    ...result,
    content: result?.content?.map((item) =>
      item.type === "image"
        ? { type: item.type, mimeType: item.mimeType, base64Bytes: item.data.length }
        : item,
    ),
  };
}

async function callTool(name, args = {}) {
  const result = await request("tools/call", {
    name,
    arguments: args,
  });
  actions.push({ name, args, result: compactResult(result) });
  if (result.isError) {
    throw new Error(`${name} failed: ${JSON.stringify(result.content)}`);
  }
  return result;
}

try {
  await request("initialize", {
    protocolVersion: "2025-06-18",
    capabilities: {},
    clientInfo: { name: "frank-ui-smoke", version: "0.1.0" },
  });
  notify("notifications/initialized");

  const toolList = await request("tools/list");
  fs.writeFileSync(
    path.join(artifactDirectory, "egui-mcp-tools.json"),
    `${JSON.stringify(toolList, null, 2)}\n`,
  );

  await callTool("attach", { host: "127.0.0.1", port: 5719, timeout_secs: 30 });
  await callTool("resize", { width: 1440, height: 900 });
  await callTool("wait_for", {
    content_contains: "Frank automation status: ready",
    timeout_secs: 30,
  });

  const initialTree = await callTool("query_tree", {
    visible_only: false,
    limit: 1000,
  });
  fs.writeFileSync(
    path.join(artifactDirectory, `${runName}-widget-tree-before.json`),
    `${JSON.stringify(initialTree.structuredContent, null, 2)}\n`,
  );
  await callTool("screenshot", {
    pixels_per_point: 1.0,
    save_path: `/artifacts/${runName}-before.png`,
  });

  await callTool("click", { label_contains: "Pane 2 image viewport" });
  if (flow === "diagnostics-after-rotation") {
    await callTool("click", { role: "Button", label_contains: "Align" });
    await callTool("click", {
      role: "Button",
      label_contains: "Auto align active to reference",
    });
    await callTool("press_key", { key: "Escape" });
    await callTool("wait_for", {
      content_contains: "Frank automation status: ready",
      timeout_secs: 30,
    });
    await callTool("click", { role: "Button", label_contains: "Align" });
    await callTool("wait_for", {
      content_contains: "Last auto align",
      timeout_secs: 10,
    });
    await callTool("click", { label_contains: "Show match diagnostics" });
    const sliderTree = await callTool("query_tree", {
      label_contains: "Fine rotation",
      visible_only: true,
    });
    const slider = sliderTree.structuredContent?.nodes?.[0];
    if (!slider?.bounds) throw new Error("Fine rotation slider has no bounds");
    const sliderCenter = {
      x: slider.bounds.x + slider.bounds.w / 2,
      y: slider.bounds.y + slider.bounds.h / 2,
    };
    await callTool("drag", {
      start: { pos: sliderCenter },
      end: { pos: { x: sliderCenter.x + 6, y: sliderCenter.y } },
      steps: 4,
    });
    await callTool("wait_for", {
      content_contains: "rotation adjusted to",
      timeout_secs: 10,
    });
    await callTool("click", { label_contains: "Show match diagnostics" });
    await callTool("click", { label_contains: "Show match diagnostics" });
  } else if (flow === "rotation") {
    await callTool("click", { role: "Button", label_contains: "Align" });
    await callTool("wait_for", {
      role: "Button",
      label_contains: "Rotate right 90°",
      timeout_secs: 10,
    });
    await callTool("click", {
      role: "Button",
      label_contains: "Rotate right 90°",
    });
    await callTool("wait_for", {
      content_contains: "rotation adjusted to +90.0°",
      timeout_secs: 10,
    });
  } else {
    throw new Error(`Unknown FRANK_SMOKE_FLOW: ${flow}`);
  }

  const finalTree = await callTool("query_tree", {
    visible_only: false,
    limit: 1000,
  });
  fs.writeFileSync(
    path.join(artifactDirectory, `${runName}-widget-tree-after.json`),
    `${JSON.stringify(finalTree.structuredContent, null, 2)}\n`,
  );
  await callTool("screenshot", {
    pixels_per_point: 1.0,
    save_path: `/artifacts/${runName}-after.png`,
  });
  await callTool("disconnect", {});

  fs.writeFileSync(
    path.join(artifactDirectory, `${runName}-actions.json`),
    `${JSON.stringify(actions, null, 2)}\n`,
  );
  process.stdout.write(`Frank ${flow} UI smoke test passed.\n`);
} catch (error) {
  fs.writeFileSync(
    path.join(artifactDirectory, `${runName}-actions.json`),
    `${JSON.stringify(actions, null, 2)}\n`,
  );
  process.stderr.write(`${error.stack ?? error}\n${stderr}`);
  process.exitCode = 1;
} finally {
  server.stdin.end();
}
