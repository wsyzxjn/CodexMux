import Cocoa

/// Bootstrap the accessory application and run its menu bar event loop.
let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
