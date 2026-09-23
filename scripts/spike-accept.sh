#!/usr/bin/env bash
# Milestone 0 spike acceptance (S1): `scripts/spike-accept.sh <compositor> <item>`, item in
# a b c f g. Runs against the LIVE session in $WAYLAND_DISPLAY; surfaces appear on screen for
# a few seconds and are destroyed on exit. Results: spike/out/<compositor>/<item>.{log,json,png}.
# Exit 0 = the item's acceptance holds (f may pass as "pending user scale change").
#
# `scripts/spike-accept.sh demo [<compositor>] <item>` runs the same items, with the same
# thresholds, against the real-API examples instead (examples/demo_bar.rs, demo_dock.rs, built
# with `debug-inject`): the H2 acceptance. The compositor defaults to $DEMO_COMPOSITOR, else
# kwin-virtual. Results: spike/out/<compositor>/demo-<item>.{log,json,png}.
set -euo pipefail
self="$(realpath "$0")"
cd "$(dirname "$0")/.."
orig=("$@")
mode=spike
if [[ "${1:-}" == demo ]]; then
  mode=demo
  shift
  if [[ $# -eq 1 ]]; then set -- "${DEMO_COMPOSITOR:-kwin-virtual}" "$1"; fi
fi
usage="usage: spike-accept.sh [demo] <compositor> <a|b|c|f|g>"
comp="${1:?$usage}"
item="${2:?$usage}"
out="spike/out/$comp"
mkdir -p "$out"
case "$mode" in
  spike) features=spike ;;
  demo) features=spike,debug-inject ;; # probe_toplevel is a spike example
esac
build() { cargo build --release -p shell-host --features "$features" --examples >&2; }
# `kwin-virtual[@scale]`: a nested headless KWin (virtual output, OpenGL compositing, own D-Bus
# session) at scale $SPIKE_SCALE (default 1.5). Used when the live session is locked, and for
# item f at exactly 1.5 without touching the user's display.
# `kwin-virtual@1.7` picks the nested output scale from the name.
if [[ "$comp" == kwin-virtual@* ]]; then export SPIKE_SCALE="${comp#kwin-virtual@}"; fi
if [[ "$comp" == kwin-virtual* && -z "${SPIKE_NESTED:-}" ]]; then
  build
  rc=0
  SPIKE_NESTED=1 dbus-run-session -- "$self" "${orig[@]}" </dev/null || rc=$?
  # D-Bus activation starts daemons (portals, fcitx5, ksecretd) on the nested bus under
  # systemd --user, some of them while KWin shuts down; they outlive dbus-run-session and
  # hold our stdout open unless killed here.
  sleep 1
  bus=$(cat "$out/.nested-bus-$item" 2>/dev/null || true)
  if [ -n "$bus" ]; then
    for p in $(pgrep -u "$USER"); do
      if { tr '\0' '\n' <"/proc/$p/environ"; } 2>/dev/null | grep -qxF "DBUS_SESSION_BUS_ADDRESS=$bus"; then
        kill "$p" 2>/dev/null || true
      fi
    done
  fi
  exit "$rc"
fi
nested_cleanup() {
  kill "$kpid" 2>/dev/null || true
  wait "$kpid" 2>/dev/null || true
}
if [[ -n "${SPIKE_NESTED:-}" ]]; then
  sock="wayland-spike-$$"
  echo "$DBUS_SESSION_BUS_ADDRESS" >"$out/.nested-bus-$item"
  kwin_wayland --virtual --no-lockscreen --no-global-shortcuts --width 1920 --height 1080 \
    --socket "$sock" >"$out/kwin-virtual-$item.log" 2>&1 &
  kpid=$!
  trap nested_cleanup EXIT
  for _ in $(seq 100); do [ -S "$XDG_RUNTIME_DIR/$sock" ] && break; sleep 0.1; done
  sleep 1.5
  export WAYLAND_DISPLAY="$sock"
  vout=$(kscreen-doctor -o 2>&1 | sed 's/\x1b\[[0-9;]*m//g' | awk '/^Output/{print $3; exit}')
  kscreen-doctor "output.$vout.scale.${SPIKE_SCALE:-1.5}" >/dev/null 2>&1
  sleep 1
else
  build
fi
export SPIKE_MODE="$mode"
export SPIKE_BIN="target/release/examples"
export SPIKE_OUT="$out" SPIKE_ITEM="$item"
python3 - <<'PY'
import json, os, queue, re, subprocess, sys, threading, time
from PIL import Image
import numpy as np

OUT, ITEM, BIN = os.environ["SPIKE_OUT"], os.environ["SPIKE_ITEM"], os.environ["SPIKE_BIN"]
# spike: the M0 examples. demo: the real-API examples, same items and thresholds (H2).
DEMO = os.environ.get("SPIKE_MODE") == "demo"
PFX = "demo-" if DEMO else ""
LOG = open(f"{OUT}/{PFX}{ITEM}.log", "w")
PROCS = []

def log(*a):
    line = " ".join(str(x) for x in a)
    print(line, file=sys.stderr); LOG.write(line + "\n"); LOG.flush()

class Proc:
    def __init__(self, name, args, env=None, prefix=""):
        # `prefix` addresses the process's primary surface: the spike takes bare commands,
        # the demos name the surface (`dock hover x y`).
        self.name, self.lines, self.prefix = name, queue.Queue(), prefix
        e = dict(os.environ); e.update(env or {})
        self.p = subprocess.Popen([f"{BIN}/{name}"] + args, stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=e)
        PROCS.append(self)
        threading.Thread(target=self._read, args=(self.p.stdout, True), daemon=True).start()
        threading.Thread(target=self._read, args=(self.p.stderr, False), daemon=True).start()
        log(f"spawn {name} {' '.join(args)} env={env or {}} pid={self.p.pid}")
    def _read(self, f, is_out):
        for line in f:
            LOG.write(f"  [{self.name}{'' if is_out else ' err'}] {line}"); LOG.flush()
            if is_out: self.lines.put(line)
    def send(self, s):
        self.p.stdin.write(s + "\n"); self.p.stdin.flush()
    def expect(self, pred, timeout=10.0):
        end = time.time() + timeout
        while time.time() < end:
            try: line = self.lines.get(timeout=max(0.01, end - time.time()))
            except queue.Empty: break
            try: v = json.loads(line)
            except ValueError: continue
            if pred(v): return v
        raise TimeoutError(f"{self.name}: expected line not seen")
    def stats(self, n=1):
        t0 = time.time(); self.send("stats")
        if DEMO:
            host = self.expect(lambda v: "host" in v)["host"]
            rows = [demo_row(self.expect(lambda v: "counters" in v), host) for _ in range(n)]
        else:
            rows = [self.expect(lambda v: "frames" in v) for _ in range(n)]
        return {r["surface"]: r for r in rows}, (time.time() - t0) * 1000
    def ctx(self):
        main = total = 0
        for tid in os.listdir(f"/proc/{self.p.pid}/task"):
            try: s = open(f"/proc/{self.p.pid}/task/{tid}/status").read()
            except OSError: continue
            v = int(re.search(r"voluntary_ctxt_switches:\s+(\d+)", s).group(1))
            total += v
            if tid == str(self.p.pid): main = v
        return main, total
    def quit(self):
        if self.p.poll() is None:
            try: self.send("quit")
            except BrokenPipeError: pass
            try: self.p.wait(3)
            except subprocess.TimeoutExpired: self.p.kill()

def demo_row(v, host):
    """A demo `FrameStats` line in the spike's flat shape, so every check reads one shape."""
    c, w, i = v["counters"], v["work"], v["interval"]
    ms = lambda us: us / 1000.0
    dims = lambda d: [d["width"], d["height"]]
    return dict(surface=v["surface"], frames=c["frames"], callbacks=c["callbacks"],
                commits=c["commits"], skipped=c["skipped"], watchdog_fired=c["watchdog_fired"],
                frame_ms_p50=ms(w["p50_us"]), frame_ms_p95=ms(w["p95_us"]),
                frame_ms_max=ms(w["max_us"]), interval_ms_p50=ms(i["p50_us"]),
                interval_ms_p95=ms(i["p95_us"]), samples=v["samples"], scale120=v["scale120"],
                logical=dims(v["logical"]), buffer=dims(v["buffer"]),
                alpha_mode=v["alpha_mode"], present_mode=v["present_mode"],
                adapter=v["adapter"], devices_created=v["devices_created"],
                vm_rss_kb=v["vm_rss_kib"], blur=host["blur"], host=host)

def surfaces(kind, args=(), env=None):
    """Start the process for `kind` ("bar", "dock" or "bar+dock"); `args` and `env` are the
    spike's (SPIKE_BLUR, --exclusive, --mode, --layer), translated for the demos."""
    args, env = list(args), dict(env or {})
    if not DEMO:
        return Proc("spike_bar", {"bar": [], "dock": ["--dock"], "bar+dock": ["--with-dock"]}[kind] + args, env)
    if "SPIKE_BLUR" in env: env["DEMO_BLUR"] = env.pop("SPIKE_BLUR")
    name = "demo_dock" if kind == "dock" else "demo_bar"
    extra = ["--with-dock"] if kind == "bar+dock" else []
    return Proc(name, extra + args, env, prefix=("dock " if kind == "dock" else "bar "))

def blur_checks(st):
    if DEMO:  # the host line: unbound, unsupported or supported
        return {"blur_bound": st["blur"] != "unbound", "caps_has_blur": st["blur"] == "supported"}
    return {"blur_bound": st["blur"].startswith("on"), "caps_has_blur": "Some(1)" in st["blur"]}

def cleanup():
    for p in PROCS: p.quit()

def output_info():
    txt = subprocess.run(["wayland-info"], capture_output=True, text=True).stdout
    lw, lh = map(int, re.search(r"logical_width: (\d+), logical_height: (\d+)", txt).groups())
    pw, ph = map(int, re.search(r"width: (\d+) px, height: (\d+) px", txt).groups())
    hz = float(re.search(r"refresh: ([\d.]+) Hz", txt).group(1))
    return dict(logical=[lw, lh], physical=[pw, ph], refresh_hz=hz)

def screenshot(path):
    if os.path.exists(path): os.remove(path)
    subprocess.run(["spectacle", "-b", "-n", "-f", "-o", path], check=True, timeout=20,
                   capture_output=True)
    for _ in range(100):
        if os.path.exists(path) and os.path.getsize(path) > 0: break
        time.sleep(0.05)
    time.sleep(0.2)
    return np.asarray(Image.open(path).convert("RGB")).astype(float)

def lum(a): return a[..., 0] * 0.299 + a[..., 1] * 0.587 + a[..., 2] * 0.114

def probe_maximized(extra=()):
    p = Proc("probe_toplevel", list(extra))
    v = p.expect(lambda v: v.get("event") == "configure" and v["sized"] and (v["maximized"] or v["fullscreen"]))
    return p, v

def bar_ready(p, n=1):
    for _ in range(100):
        s, _ = p.stats(n)
        if all(r["frames"] >= 1 for r in s.values()) and len(s) == n: return s
        time.sleep(0.05)
    raise TimeoutError("surface never presented")

def blur_setting():
    r = subprocess.run(["kreadconfig6", "--file", "kwinrc", "--group", "Plugins", "--key",
                        "blurEnabled"], capture_output=True, text=True).stdout.strip()
    loaded = subprocess.run(["gdbus", "call", "--session", "--dest", "org.kde.KWin",
                             "--object-path", "/Effects", "--method",
                             "org.kde.kwin.Effects.isEffectLoaded", "blur"],
                            capture_output=True, text=True).stdout.strip()
    return dict(kwinrc_blurEnabled=r or "(unset: default on)", effect_loaded=loaded)

def item_a():
    info = output_info()
    base, v0 = probe_maximized(); time.sleep(0.3); base.quit()
    bar = surfaces("bar", [], {"SPIKE_BLUR": "0"}); st = bar_ready(bar)["bar"]
    time.sleep(0.3)
    probe, v1 = probe_maximized(); time.sleep(1.5)
    img = screenshot(f"{OUT}/{PFX}a.png")
    probe.quit(); bar.quit()
    s = img.shape[1] / info["logical"][0]
    px = lambda x, y: img[int(round(y * s)), int(round(x * s))]
    bar_px = px(10, 10)
    y0, x0 = int(round(40 * s)), int(round(10 * s))
    patch = lum(img[y0 - 4:y0 + 4, x0 - 4:x0 + 4])
    checks = {
        "height_delta_is_32": v0["height"] - v1["height"] == 32,
        "bar_pixel_is_magenta": bool(bar_px[0] > bar_px[1] + 60 and bar_px[2] > bar_px[1]),
        "checker_at_10_40": bool(patch.min() < 40 and patch.max() > 215),
    }
    return dict(output=info, baseline_max_height=v0["height"], with_bar_max_height=v1["height"],
                output_h_minus_32=info["logical"][1] - 32, bar_pixel_10_10=bar_px.tolist(),
                checker_patch_min_max=[patch.min(), patch.max()], bar_stats=st,
                screenshot_scale=s, checks=checks, pass_=all(checks.values()))

def bar_variance(blur):
    bar = surfaces("bar", ["--exclusive=0"], {"SPIKE_BLUR": blur}); st = bar_ready(bar)["bar"]
    probe, _ = probe_maximized(); time.sleep(1.5)
    img = screenshot(f"{OUT}/{PFX}b_blur{blur}.png")
    probe.quit(); bar.quit(); time.sleep(0.3)
    s = img.shape[1] / output_info()["logical"][0]
    region = lum(img[int(6 * s):int(26 * s), int(400 * s):int(1400 * s)])
    return float(region.var()), st

def item_b():
    setting = blur_setting()
    v0, st0 = bar_variance("0"); v1, st1 = bar_variance("1")
    drop = 1 - v1 / v0 if v0 > 0 else 0.0
    os.replace(f"{OUT}/{PFX}b_blur1.png", f"{OUT}/{PFX}b.png")
    checks = {**blur_checks(st1), "variance_drop_ge_80pct": drop >= 0.8}
    return dict(kwin_blur=setting, variance_blur0=v0, variance_blur1=v1, drop=drop,
                blur0_stats=st0, blur1_stats=st1, checks=checks, pass_=all(checks.values()))

def sweep(p, target=None, secs=1.0, y=64, x0=40, x1=720):
    target = p.prefix if target is None else target
    n = int(secs * 120)
    for i in range(n + 1):
        p.send(f"{target}hover {x0 + (x1 - x0) * i / n:.1f} {y}"); time.sleep(secs / n)

def dock_run(mode, refresh, present="mailbox"):
    # Bottom layer so an ordinary toplevel really covers it (KWin keeps top-layer surfaces
    # above fullscreen windows); the occluder must stop our frame callbacks to mean anything.
    d = surfaces("dock", [f"--mode={mode}", "--layer=bottom"],
                 {"SPIKE_PRESENT": present}); bar_ready(d)
    s0, _ = d.stats()
    sweep(d); d.send(f"{d.prefix}leave"); time.sleep(0.4)
    s1, _ = d.stats(); a, b = s0["dock"], s1["dock"]
    frames, cbs = b["frames"] - a["frames"], b["callbacks"] - a["callbacks"]
    c0 = d.ctx(); time.sleep(5.0); c1 = d.ctx()
    s2, _ = d.stats(); idle_frames = s2["dock"]["frames"] - b["frames"]
    idle_commits = s2["dock"]["commits"] - b["commits"]
    # Occlusion: a fullscreen toplevel over the dock; commands must still be answered promptly.
    occ, _ = probe_maximized(); time.sleep(0.8)
    so0, _ = d.stats(); sweep(d, secs=0.5); rtts = [d.stats()[1] for _ in range(5)]
    so1, _ = d.stats(); d.send(f"{d.prefix}leave"); occ.quit(); time.sleep(0.5)
    sweep(d, secs=0.3); d.send(f"{d.prefix}leave"); time.sleep(0.4); sr, _ = d.stats(); d.quit()
    budget = 1000.0 / refresh
    checks = {"frames_ge_0.9_callbacks": frames >= 0.9 * cbs and frames > 10,
              "p95_lt_refresh": b["frame_ms_p95"] < budget,
              "idle_frames_0": idle_frames == 0, "idle_commits_0": idle_commits == 0,
              "idle_ctx_le_5": c1[0] - c0[0] <= 5,
              "occluded_no_block": max(rtts) < 250,
              "recovers_after_occlusion": sr["dock"]["frames"] > so1["dock"]["frames"]}
    return dict(mode=mode, present_requested=present, sweep=dict(frames=frames, callbacks=cbs, frame_ms_p50=b["frame_ms_p50"],
                frame_ms_p95=b["frame_ms_p95"], frame_ms_max=b["frame_ms_max"],
                interval_ms_p50=b["interval_ms_p50"], interval_ms_p95=b["interval_ms_p95"],
                budget_ms=budget),
                idle=dict(frames=idle_frames, commits=idle_commits,
                          ctx_main=c1[0] - c0[0], ctx_all_threads=c1[1] - c0[1]),
                occluded=dict(frames=so1["dock"]["frames"] - so0["dock"]["frames"],
                              callbacks=so1["dock"]["callbacks"] - so0["dock"]["callbacks"],
                              stats_rtt_ms=rtts, present_mode=b["present_mode"],
                              # KWin keeps sending frame callbacks to a covered surface, so this
                              # is informational: blocking could only happen where they stop.
                              compositor_stopped_callbacks=so1["dock"]["callbacks"] - so0["dock"]["callbacks"] <= 1),
                stats=b, checks=checks, pass_=all(checks.values()))

def item_c():
    hz = output_info()["refresh_hz"]
    runs = [dock_run(m, hz) for m in ("transform", "width")]
    fifo = dock_run("transform", hz, present="fifo")
    return dict(refresh_hz=hz, runs=runs, fifo_run=fifo, pass_=all(r["pass_"] for r in runs))

def runs_of(row):
    dark = row < 128; runs, n = [], 1
    for a, b in zip(dark[:-1], dark[1:]):
        if a == b: n += 1
        else: runs.append(n); n = 1
    runs.append(n); return runs

def item_f():
    kd = subprocess.run(["kscreen-doctor", "-o"], capture_output=True, text=True).stdout
    kscale = float(re.search(r"Scale: ([\d.]+)", re.sub(r"\x1b\[[0-9;]*m", "", kd)).group(1))
    bar = surfaces("bar"); st = bar_ready(bar)["bar"]
    cap = f"{os.path.abspath(OUT)}/{PFX}f_capture.png"
    bar.send(f"{bar.prefix}capture {cap}"); bar.expect(lambda v: "capture" in v)
    time.sleep(0.5); img = screenshot(f"{OUT}/{PFX}f.png"); bar.quit()
    # Recorded, not checked; the host always binds fractional-scale when offered.
    st_int = None
    if not DEMO:
        fb = Proc("spike_bar", [], {"SPIKE_NO_FRACTIONAL": "1"}); st_int = bar_ready(fb)["bar"]; fb.quit()
    s = st["scale120"] / 120
    buf = [int((l * st["scale120"] + 60) // 120) for l in st["logical"]]
    y = int(16 * s); x0, x1 = int(round(200 * s)), int(round(280 * s))
    ca = np.asarray(Image.open(cap).convert("RGBA")).astype(float)
    c = ca[..., :3]
    crow, srow = lum(c[y, x0:x1]), lum(img[y, x0:x1])
    # Anti-aliased edge pixels are partly transparent (checker over the translucent bar) and
    # the compositor blends them over whatever is behind, so compare opaque pixels only: they
    # must match 1:1 if nothing resampled the buffer.
    opaque = ca[y, x0:x1, 3] >= 255
    runs = runs_of(crow)[1:-1]
    # Cell edges fall at 2*k*s physical px; with coverage >= 50% counted as dark, every run is
    # floor(2s) or ceil(2s) px long (exactly 3 at s = 1.5; 3 or 4, averaging 3.4, at 1.7).
    allowed = sorted({int(np.floor(2 * s + 1e-9)), int(np.ceil(2 * s - 1e-9))})
    aa = int(((crow > 16) & (crow < 239)).sum())
    checks = {"preferred_scale_matches_kscreen": st["scale120"] == round(kscale * 120),
              "buffer_is_round_logical_x_scale": st["buffer"] == buf,
              "screen_row_equals_buffer_row": float(np.abs(crow - srow)[opaque].max()) <= 8,
              "checker_runs_in_allowed": len(runs) > 10 and all(r in allowed for r in runs),
              "checker_mean_run_is_2s": abs(float(np.mean(runs)) - 2 * s) < 0.1}
    return dict(kscreen_scale=kscale, stats=st, expected_buffer=buf, checker_runs=runs,
                allowed_runs=allowed, mean_run=float(np.mean(runs)), antialiased_px=aa,
                max_row_diff_opaque=float(np.abs(crow - srow)[opaque].max()),
                max_row_diff_all=float(np.abs(crow - srow).max()), opaque_px=int(opaque.sum()),
                surface_covers_output=int(img.shape[1]) <= st["buffer"][0], integer_fallback_stats=st_int,
                checks=checks, pass_=all(checks.values()))

def item_g():
    solo = surfaces("bar"); rss_solo = bar_ready(solo)["bar"]["vm_rss_kb"]; solo.quit()
    p = surfaces("bar+dock"); s0 = bar_ready(p, 2)
    sweep(p, target="dock ", secs=0.5); p.send("dock leave"); time.sleep(0.4)
    s1, _ = p.stats(2); p.quit()
    d = {k: s1[k]["frames"] - s0[k]["frames"] for k in s1}
    checks = {"one_device": all(r["devices_created"] == 1 for r in s1.values()),
              "same_adapter": len({r["adapter"] for r in s1.values()}) == 1,
              "dock_counted": d["dock"] > 10, "bar_untouched": d["bar"] == 0}
    return dict(stats=s1, frame_deltas=d, vm_rss_kb_bar_only=rss_solo,
                vm_rss_kb_bar_and_dock=s1["bar"]["vm_rss_kb"],
                vm_rss_kb_second_surface=s1["bar"]["vm_rss_kb"] - rss_solo,
                checks=checks, pass_=all(checks.values()))

try:
    res = {"a": item_a, "b": item_b, "c": item_c, "f": item_f, "g": item_g}[ITEM]()
finally:
    cleanup()
res["pass"] = bool(res.pop("pass_"))
json.dump(res, open(f"{OUT}/{PFX}{ITEM}.json", "w"), indent=2, default=lambda o: o.tolist() if hasattr(o, "tolist") else str(o))
log(json.dumps(res, default=str))
sys.exit(0 if res["pass"] else 1)
PY
