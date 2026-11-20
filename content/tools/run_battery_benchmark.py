import asyncio
import json
import subprocess
import sys
import time
import websockets

def get_battery_stats():
    out = subprocess.check_output(["adb", "shell", "dumpsys", "battery"]).decode()
    level = None
    temp = None
    voltage = None
    cc = None
    for line in out.splitlines():
        line = line.strip()
        if line.startswith("level:"):
            level = int(line.split(":")[1])
        elif line.startswith("temperature:"):
            temp = float(line.split(":")[1]) / 10.0
        elif line.startswith("voltage:"):
            voltage = int(line.split(":")[1])
        elif "cc:" in line and "present:true" in line:
            # e.g. cc:4610000
            parts = line.split()
            for p in parts:
                if p.startswith("cc:"):
                    cc = int(p.split(":")[1].rstrip(","))
    return {"level": level, "temp": temp, "voltage": voltage, "cc": cc}

def get_mem_stats():
    out = subprocess.check_output(["adb", "shell", "dumpsys", "meminfo", "com.mila.langualinker.dev"]).decode()
    pss = 0
    rss = 0
    for line in out.splitlines():
        if "TOTAL PSS:" in line:
            parts = line.split()
            pss = int(parts[2])
            if "TOTAL RSS:" in line:
                rss = int(parts[5])
    return {"pss_mb": round(pss / 1024, 1), "rss_mb": round(rss / 1024, 1)}

async def run_benchmark(count=20):
    targets_raw = subprocess.check_output(["curl", "-s", "http://127.0.0.1:9222/json"]).decode()
    targets = json.loads(targets_raw)
    ws_url = targets[0]["webSocketDebuggerUrl"]

    print(f"Connecting to DevTools at {ws_url}...")
    start_batt = get_battery_stats()
    start_mem = get_mem_stats()
    print(f"Starting Battery: {start_batt}")
    print(f"Starting Memory: {start_mem}")

    results = []

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

        # Get cards to test
        # We can test cards 1, 3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 23, 25, 27, 29, 31, 33, 35, 37, 39
        card_ids = [2 * i + 1 for i in range(count)]

        for idx, cid in enumerate(card_ids, 1):
            print(f"\n--- [{idx}/{count}] Testing Card {cid} ---")
            # Navigate to card
            await eval_js(f"window.location.hash = '#/card/{cid}'; window.location.reload();")
            await asyncio.sleep(1.0)

            # Read word
            word = await eval_js("document.querySelector('h1, h2, .front')?.innerText || 'unknown'")
            print(f"Card word: {word}")

            # Click generate
            t0 = time.time()
            btn_clicked = await eval_js("""
                (() => {
                    const btn = Array.from(document.querySelectorAll('button')).find(b => b.innerText.includes('Generate new association') || b.innerText.includes('Generuj'));
                    if (btn) {
                        btn.click();
                        return true;
                    }
                    return false;
                })()
            """)
            if not btn_clicked:
                print("Generate button not found!")
                continue

            # Wait for generation to complete
            while True:
                await asyncio.sleep(0.3)
                state = await eval_js("""
                    (() => {
                        const text = document.body.innerText;
                        if (text.includes('Generating…') || text.includes('Generowanie…') || text.includes('In progress')) {
                            return 'generating';
                        }
                        const btn = Array.from(document.querySelectorAll('button')).find(b => b.innerText.includes('Generate new association') || b.innerText.includes('Generuj'));
                        if (btn && !btn.disabled) return 'idle';
                        return 'waiting';
                    })()
                """)
                elapsed = time.time() - t0
                if state == 'idle' and elapsed > 0.5:
                    break
                if elapsed > 45:
                    print("Generation timed out!")
                    break

            duration = round(time.time() - t0, 2)
            mem = get_mem_stats()
            print(f"Duration: {duration}s | PSS: {mem['pss_mb']} MB, RSS: {mem['rss_mb']} MB")
            results.append({"card_id": cid, "word": word, "duration_s": duration, "mem": mem})

    end_batt = get_battery_stats()
    end_mem = get_mem_stats()

    print("\n================ BENCHMARK RESULTS ================")
    print(f"Total runs: {len(results)}")
    durations = [r["duration_s"] for r in results]
    avg_dur = sum(durations) / len(durations)
    min_dur = min(durations)
    max_dur = max(durations)
    print(f"Latency: Avg={avg_dur:.2f}s, Min={min_dur:.2f}s, Max={max_dur:.2f}s")
    print(f"Memory: PSS={end_mem['pss_mb']} MB, RSS={end_mem['rss_mb']} MB")
    print(f"Battery Start: {start_batt}")
    print(f"Battery End:   {end_batt}")
    if start_batt["level"] is not None and end_batt["level"] is not None:
        drop_pct = start_batt["level"] - end_batt["level"]
        print(f"Battery level drop: {drop_pct}%")
    if start_batt["cc"] is not None and end_batt["cc"] is not None:
        drop_uah = start_batt["cc"] - end_batt["cc"]
        print(f"Charge counter drop: {drop_uah} µAh ({drop_uah / 1000:.1f} mAh)")
    print(f"Temperature change: {start_batt['temp']}°C -> {end_batt['temp']}°C")

    with open("target/benchmark_results.json", "w") as f:
        json.dump({
            "start_battery": start_batt,
            "end_battery": end_batt,
            "start_mem": start_mem,
            "end_mem": end_mem,
            "runs": results,
            "summary": {
                "count": len(results),
                "avg_s": avg_dur,
                "min_s": min_dur,
                "max_s": max_dur
            }
        }, f, indent=2)

if __name__ == "__main__":
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 20
    asyncio.run(run_benchmark(count))
