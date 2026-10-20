// Edgeway — one keyboard, one trackpad, every computer.
// Copyright (c) 2026 Rafe Uddaraj. MIT License.

import AppKit

let launchOptions = LaunchOptions.parse(CommandLine.arguments)
let app = NSApplication.shared
let delegate = AppDelegate(options: launchOptions)
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
