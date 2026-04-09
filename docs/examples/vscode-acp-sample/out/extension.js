"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = __importStar(require("vscode"));
const cp = __importStar(require("child_process"));
let proc = null;
let buf = '';
let nextId = 1;
const pending = new Map();
let currentSessionId = null;
const out = vscode.window.createOutputChannel('ZeroClaw ACP');
function startServer() {
    if (proc) {
        return;
    }
    proc = cp.spawn('zeroclaw', ['acp'], { stdio: ['pipe', 'pipe', 'pipe'] });
    proc.stdout.on('data', (chunk) => {
        buf += chunk.toString('utf8');
        let idx;
        while ((idx = buf.indexOf('\n')) >= 0) {
            const line = buf.slice(0, idx).trim();
            buf = buf.slice(idx + 1);
            if (!line)
                continue;
            try {
                const msg = JSON.parse(line);
                if (msg && msg.method === 'session/event') {
                    const p = msg.params || {};
                    const typ = p.type || 'event';
                    if (typ === 'chunk' || typ === 'thinking') {
                        out.append(p.content ?? '');
                    }
                    else if (typ === 'tool_call') {
                        out.appendLine(`[tool_call] ${p.name} ${JSON.stringify(p.args ?? {})}`);
                    }
                    else if (typ === 'tool_result') {
                        out.appendLine(`[tool_result] ${p.name} ${JSON.stringify(p.output ?? {})}`);
                    }
                    else {
                        out.appendLine(`[event] ${JSON.stringify(p)}`);
                    }
                }
                else if (Object.prototype.hasOwnProperty.call(msg, 'id')) {
                    const idNum = typeof msg.id === 'number' ? msg.id : Number(msg.id);
                    const done = pending.get(idNum);
                    if (done) {
                        pending.delete(idNum);
                        done(msg);
                    }
                }
            }
            catch (e) {
                // ignore parse errors
            }
        }
    });
    proc.stderr.on('data', (chunk) => {
        out.appendLine(`[acp stderr] ${chunk.toString('utf8')}`);
    });
    proc.on('exit', (code, signal) => {
        out.appendLine(`ACP exited (code=${code} signal=${signal})`);
        proc = null;
        buf = '';
        pending.clear();
    });
}
function sendRpc(method, params, id) {
    startServer();
    if (!proc || !proc.stdin) {
        return Promise.reject(new Error('ACP process not running'));
    }
    const reqId = id ?? nextId++;
    const payload = { jsonrpc: '2.0', method, params, id: reqId };
    proc.stdin.write(JSON.stringify(payload) + '\n');
    return new Promise((resolve) => pending.set(reqId, resolve));
}
async function ensureSession() {
    await sendRpc('initialize', {});
    const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ||
        process.cwd();
    const resp = await sendRpc('session/new', { cwd });
    const sid = resp?.result?.sessionId ?? resp?.result?.sessionID ?? resp?.result?.session_id;
    if (!sid) {
        throw new Error('Failed to create session');
    }
    currentSessionId = sid;
    vscode.window.showInformationMessage(`ZeroClaw session created: ${sid}`);
}
async function activate(context) {
    context.subscriptions.push(vscode.commands.registerCommand('zeroclawAcp.newSession', async () => {
        try {
            await ensureSession();
        }
        catch (e) {
            vscode.window.showErrorMessage(`New session failed: ${e?.message ?? e}`);
        }
    }), vscode.commands.registerCommand('zeroclawAcp.prompt', async () => {
        try {
            if (!currentSessionId) {
                await ensureSession();
            }
            const prompt = await vscode.window.showInputBox({ prompt: 'Enter prompt' });
            if (!prompt)
                return;
            out.show(true);
            const resp = await sendRpc('session/prompt', { sessionId: currentSessionId, prompt });
            const content = resp?.result?.content;
            if (content) {
                out.appendLine('\n--- Final Result ---');
                out.append(typeof content === 'string' ? content : JSON.stringify(content, null, 2));
                out.appendLine('\n--------------------');
            }
        }
        catch (e) {
            vscode.window.showErrorMessage(`Prompt failed: ${e?.message ?? e}`);
        }
    }), vscode.commands.registerCommand('zeroclawAcp.stopSession', async () => {
        try {
            if (!currentSessionId) {
                vscode.window.showInformationMessage('No active session.');
                return;
            }
            const sid = currentSessionId;
            await sendRpc('session/stop', { sessionId: sid });
            currentSessionId = null;
            vscode.window.showInformationMessage(`Session stopped: ${sid}`);
        }
        catch (e) {
            vscode.window.showErrorMessage(`Stop failed: ${e?.message ?? e}`);
        }
    }), { dispose() { if (proc) {
            proc.kill();
            proc = null;
        } } });
}
function deactivate() {
    if (proc) {
        proc.kill();
        proc = null;
    }
}
//# sourceMappingURL=extension.js.map