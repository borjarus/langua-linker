import asyncio
import json
import sys
import websockets

async def run_cdp(expr):
    targets = json.loads(sys.stdin.read())
    ws_url = targets[0]["webSocketDebuggerUrl"]
    async with websockets.connect(ws_url) as ws:
        msg_id = 1
        req = {"id": msg_id, "method": "Runtime.evaluate", "params": {"expression": expr, "awaitPromise": True, "returnByValue": True}}
        await ws.send(json.dumps(req))
        while True:
            raw = await ws.recv()
            data = json.loads(raw)
            if data.get("id") == msg_id:
                res = data.get("result", {}).get("result", {})
                print(json.dumps(res.get("value", res.get("description", ""))))
                break

if __name__ == "__main__":
    asyncio.run(run_cdp(sys.argv[1]))
