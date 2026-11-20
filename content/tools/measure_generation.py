import asyncio
import json
import sys
import time
import websockets

async def measure():
    targets = json.loads(sys.stdin.read())
    ws_url = targets[0]["webSocketDebuggerUrl"]
    async with websockets.connect(ws_url) as ws:
        msg_id = 0
        async def eval_js(expr):
            nonlocal msg_id
            msg_id += 1
            await ws.send(json.dumps({
                "id": msg_id,
                "method": "Runtime.evaluate",
                "params": {"expression": expr, "awaitPromise": True, "returnByValue": True}
            }))
            while True:
                raw = await ws.recv()
                data = json.loads(raw)
                if data.get("id") == msg_id:
                    res = data.get("result", {}).get("result", {})
                    return res.get("value")

        # Find the button and click it
        t0 = time.time()
        print("Clicking Generate new association...")
        btn_found = await eval_js("""
            (() => {
                const btn = Array.from(document.querySelectorAll('button')).find(b => b.innerText.includes('Generate new association') || b.innerText.includes('Generuj'));
                if (btn) {
                    btn.click();
                    return true;
                }
                return false;
            })()
        """)
        print(f"Button found and clicked: {btn_found}")
        if not btn_found:
            return

        # Poll every 200ms until generation finishes
        last_state = ""
        while True:
            await asyncio.sleep(0.2)
            state = await eval_js("""
                (() => {
                    const text = document.body.innerText;
                    if (text.includes('Generating…') || text.includes('Generowanie…') || text.includes('In progress')) {
                        return 'generating';
                    }
                    const btn = Array.from(document.querySelectorAll('button')).find(b => b.innerText.includes('Generate new association') || b.innerText.includes('Generuj'));
                    if (btn && !btn.disabled) {
                        return 'idle';
                    }
                    return 'waiting';
                })()
            """)
            if state != last_state:
                print(f"[{time.time() - t0:.2f}s] State: {state}")
                last_state = state
            if state == 'idle' and (time.time() - t0 > 0.5):
                break
            if time.time() - t0 > 30:
                print("Timed out after 30s")
                break

        elapsed = time.time() - t0
        print(f"Total time elapsed: {elapsed:.2f}s")
        # Get associations on screen
        assoc_text = await eval_js("""
            (() => {
                const el = document.querySelector('section.associations, [aria-label=\"Associations\"], div.associations');
                return el ? el.innerText : document.body.innerText.slice(0, 800);
            })()
        """)
        print("Result text:\n", assoc_text)

if __name__ == "__main__":
    asyncio.run(measure())
