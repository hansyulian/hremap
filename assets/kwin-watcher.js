workspace.windowActivated.connect(function (window) {
  if (!window) return;
  callDBus(
    "org.kde.WindowWatcher",
    "/WindowWatcher",
    "org.kde.WindowWatcher",
    "windowActivated",
    window.caption || "",
    window.resourceClass || "",
    window.resourceName || "",
    window.pid || 0
  );
});