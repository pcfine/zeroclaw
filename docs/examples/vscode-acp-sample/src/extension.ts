import * as vscode from 'vscode';
import * as cp from 'child_process';

type Json = any;

let proc: cp.ChildProcessWithoutNullStreams | null = null;
let buf = '';
let nextId = 1;
const pending = new Map<number, (msg: Json) => void>();
let currentSessionId: string | null = null;

const out = vscode.window.createOutputChannel('ZeroClaw ACP');

function startServer() {
  if (proc) { return; }
  proc = cp.spawn('zeroclaw', ['acp'], { stdio: ['pipe', 'pipe', 'pipe'] });

  proc.stdout.on('data', (chunk: Buffer) => {
    buf += chunk.toString('utf8');
    let idx: number;
    while ((idx = buf.indexOf('\n')) >= 0) {
      const line = buf.slice(0, idx).trim();
      buf = buf.slice(idx + 1);
      if (!line) continue;
      try {
        const msg = JSON.parse(line);
        if (msg && msg.method === 'session/event') {
          const p = msg.params || {};
          const typ = p.type || 'event';
          if (typ === 'chunk' || typ === 'thinking') {
            out.append(p.content ?? '');
          } else if (typ === 'tool_call') {
            out.appendLine(`[tool_call] ${p.name} ${JSON.stringify(p.args ?? {})}`);
          } else if (typ === 'tool_result') {
            out.appendLine(`[tool_result] ${p.name} ${JSON.stringify(p.output ?? {})}`);
          } else {
            out.appendLine(`[event] ${JSON.stringify(p)}`);
          }
        } else if (Object.prototype.hasOwnProperty.call(msg, 'id')) {
          const idNum = typeof msg.id === 'number' ? msg.id : Number(msg.id);
          const done = pending.get(idNum);
          if (done) {
            pending.delete(idNum);
            done(msg);
          }
        }
      } catch (e) {
        // ignore parse errors
      }
    }
  });

  proc.stderr.on('data', (chunk: Buffer) => {
    out.appendLine(`[acp stderr] ${chunk.toString('utf8')}`);
  });

  proc.on('exit', (code, signal) => {
    out.appendLine(`ACP exited (code=${code} signal=${signal})`);
    proc = null;
    buf = '';
    pending.clear();
  });
}

function sendRpc(method: string, params: Json, id?: number): Promise<Json> {
  startServer();
  if (!proc || !proc.stdin) {
    return Promise.reject(new Error('ACP process not running'));
  }
  const reqId = id ?? nextId++;
  const payload = { jsonrpc: '2.0', method, params, id: reqId };
  proc.stdin.write(JSON.stringify(payload) + '\n');
  return new Promise<Json>((resolve) => pending.set(reqId, resolve));
}

async function ensureSession() {
  await sendRpc('initialize', {});
  const cwd =
    vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ||
    process.cwd();
  const resp = await sendRpc('session/new', { cwd });
  const sid = resp?.result?.sessionId ?? resp?.result?.sessionID ?? resp?.result?.session_id;
  if (!sid) { throw new Error('Failed to create session'); }
  currentSessionId = sid;
  vscode.window.showInformationMessage(`ZeroClaw session created: ${sid}`);
}

export async function activate(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.commands.registerCommand('zeroclawAcp.newSession', async () => {
      try {
        await ensureSession();
      } catch (e: any) {
        vscode.window.showErrorMessage(`New session failed: ${e?.message ?? e}`);
      }
    }),

    vscode.commands.registerCommand('zeroclawAcp.prompt', async () => {
      try {
        if (!currentSessionId) {
          await ensureSession();
        }
        const prompt = await vscode.window.showInputBox({ prompt: 'Enter prompt' });
        if (!prompt) return;
        out.show(true);
        const resp = await sendRpc('session/prompt', { sessionId: currentSessionId, prompt });
        const content = resp?.result?.content;
        if (content) {
          out.appendLine('\n--- Final Result ---');
          out.append(typeof content === 'string' ? content : JSON.stringify(content, null, 2));
          out.appendLine('\n--------------------');
        }
      } catch (e: any) {
        vscode.window.showErrorMessage(`Prompt failed: ${e?.message ?? e}`);
      }
    }),

    vscode.commands.registerCommand('zeroclawAcp.stopSession', async () => {
      try {
        if (!currentSessionId) {
          vscode.window.showInformationMessage('No active session.');
          return;
        }
        const sid = currentSessionId;
        await sendRpc('session/stop', { sessionId: sid });
        currentSessionId = null;
        vscode.window.showInformationMessage(`Session stopped: ${sid}`);
      } catch (e: any) {
        vscode.window.showErrorMessage(`Stop failed: ${e?.message ?? e}`);
      }
    }),

    { dispose() { if (proc) { proc.kill(); proc = null; } } }
  );
}

export function deactivate() {
  if (proc) { proc.kill(); proc = null; }
}
