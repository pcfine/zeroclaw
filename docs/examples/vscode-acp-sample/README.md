# ZeroClaw ACP VS Code Sample

Minimal VS Code extension that talks to `zeroclaw acp` over JSON-RPC via stdio.

## Prerequisites
- `zeroclaw` binary available on PATH (or adjust spawn path in `src/extension.ts`)
- Node.js 18+

## Install & Run
```bash
npm install
npm run compile
```

### Debug (F5)
- Open this folder in VS Code
- Press F5 to launch an Extension Development Host
- Use commands:
  - "ZeroClaw ACP: New Session"
  - "ZeroClaw ACP: Prompt"
  - "ZeroClaw ACP: Stop Session"

## Notes
- Messages are newline-delimited JSON-RPC 2.0
- Streaming events appear in the "ZeroClaw ACP" output panel
- Session idle timeout defaults to 1h; change by launching `zeroclaw acp --session-timeout <secs>` if you fork the spawn
