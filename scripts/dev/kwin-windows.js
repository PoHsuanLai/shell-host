// Prints one line per window of the headless KWin to its log (scripts/dev/out/kwin-virtual.log):
//   WINDOW {"class":..,"caption":..,"frame":[x,y,w,h],"buffer":[x,y,w,h],"maximize":0-3,...}
// `frame` is the window geometry (for a client-decorated window, what it set with
// xdg_surface.set_window_geometry), `buffer` the whole surface. Run it with
// scripts/dev/kwin-windows.sh, which loads, runs and unloads it and prints the lines.
for (const w of workspace.windowList()) {
    const f = w.frameGeometry, b = w.bufferGeometry;
    print("WINDOW " + JSON.stringify({
        class: w.resourceClass,
        caption: w.caption,
        frame: [f.x, f.y, f.width, f.height],
        buffer: [b.x, b.y, b.width, b.height],
        maximize: w.maximizeMode,
        fullscreen: w.fullScreen,
        active: w.active,
    }));
}
