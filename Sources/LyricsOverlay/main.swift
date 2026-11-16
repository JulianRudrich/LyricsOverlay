import AppKit
import Carbon
import SwiftUI

final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate, NSMenuDelegate {
    let model = PlayerModel(isDemo: CommandLine.arguments.contains("--demo"))
    var overlay: NSPanel!
    var settings: NSWindow!
    var statusItem: NSStatusItem!
    var statusMenu: NSMenu!
    var clickThrough = false
    var clickItem: NSMenuItem!
    var visibilityItem: NSMenuItem!
    private var hotKey: EventHotKeyRef?
    private var hotKeyHandler: EventHandlerRef?

    func applicationDidFinishLaunching(_ notification: Notification) {
        UserDefaults.standard.register(defaults: ["fontSize": 25.0, "opacity": 0.78, "overlayOpacity": 1.0, "offset": 0.0, "overlayWidth": 400.0, "overlayHeight": 240.0, "lookupOnline": true])
        NSApp.setActivationPolicy(.accessory)
        overlay = NSPanel(contentRect: NSRect(x: 0, y: 0, width: preferredSize.width, height: preferredSize.height), styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        overlay.isOpaque = false; overlay.backgroundColor = .clear; overlay.hasShadow = true
        overlay.level = .floating; overlay.isMovableByWindowBackground = true
        overlay.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        overlay.contentView = NSHostingView(rootView: OverlayView(model: model, showSettings: { [weak self] in self?.showSettings() }))
        overlay.delegate = self
        if let saved = UserDefaults.standard.string(forKey: "windowFrame"), NSRectFromString(saved).width > 0 {
            let rect = NSRectFromString(saved)
            if let screen = NSScreen.screens.first(where: { $0.visibleFrame.intersects(rect) }) {
                let resized = NSRect(x: rect.minX, y: rect.maxY - preferredSize.height,
                                     width: preferredSize.width, height: preferredSize.height)
                overlay.setFrame(fitted(resized, in: screen), display: true)
            } else {
                position("topRight")
            }
        } else { position("topRight") }
        overlay.orderFrontRegardless()
        buildMenu()
        setClickThrough(UserDefaults.standard.bool(forKey: "clickThrough"))
        registerHotKey()
        model.start()
    }
    private func registerHotKey() {
        var eventType = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        let context = Unmanaged.passUnretained(self).toOpaque()
        let handlerStatus = InstallEventHandler(GetApplicationEventTarget(), { _, event, context in
            guard let event, let context else { return OSStatus(eventNotHandledErr) }
            var eventID = EventHotKeyID()
            let status = GetEventParameter(event, EventParamName(kEventParamDirectObject), EventParamType(typeEventHotKeyID), nil, MemoryLayout<EventHotKeyID>.size, nil, &eventID)
            guard status == noErr, eventID.signature == 0x4C595243, eventID.id == 1 else { return OSStatus(eventNotHandledErr) }
            Unmanaged<AppDelegate>.fromOpaque(context).takeUnretainedValue().toggleVisibility()
            return noErr
        }, 1, &eventType, context, &hotKeyHandler)
        guard handlerStatus == noErr else {
            showHotKeyError()
            return
        }
        let id = EventHotKeyID(signature: 0x4C595243, id: 1)
        let status = RegisterEventHotKey(UInt32(kVK_ANSI_L), UInt32(optionKey | shiftKey), id, GetApplicationEventTarget(), 0, &hotKey)
        if status != noErr { showHotKeyError() }
    }
    private func showHotKeyError() {
        let alert = NSAlert()
        alert.messageText = "Hotkey nicht verfügbar"
        alert.informativeText = "⌥ ⇧ L konnte nicht registriert werden. Möglicherweise verwendet eine andere App diese Tastenkombination. Das Overlay lässt sich weiterhin über das Menü ein- und ausblenden."
        alert.runModal()
    }
    func applicationWillTerminate(_ notification: Notification) {
        if let hotKey { UnregisterEventHotKey(hotKey) }
        if let hotKeyHandler { RemoveEventHandler(hotKeyHandler) }
    }
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        setClickThrough(false)
        overlay.orderFrontRegardless()
        visibilityItem.title = "Overlay ausblenden"
        showSettings()
        return true
    }
    func windowDidMove(_ notification: Notification) { saveFrame() }
    private func saveFrame() { if overlay != nil { UserDefaults.standard.set(NSStringFromRect(overlay.frame), forKey: "windowFrame") } }
    private var preferredSize: NSSize {
        let preferences = UserDefaults.standard
        let layout = preferences.string(forKey: "overlayLayout").flatMap(OverlayLayout.init(rawValue:)) ?? .context
        return NSSize(width: preferences.double(forKey: "overlayWidth"),
                      height: layout.height(for: preferences.double(forKey: "overlayHeight")))
    }

    private func fitted(_ frame: NSRect, in screen: NSScreen) -> NSRect {
        let area = screen.visibleFrame.insetBy(dx: 16, dy: 16)
        let width = min(frame.width, area.width)
        let height = min(frame.height, area.height)
        return NSRect(x: min(max(frame.minX, area.minX), area.maxX - width),
                      y: min(max(frame.minY, area.minY), area.maxY - height), width: width, height: height)
    }

    func resize() {
        guard let screen = overlay.screen ?? NSScreen.main else { return }
        let area = screen.visibleFrame.insetBy(dx: 16, dy: 16)
        let old = overlay.frame
        let size = preferredSize
        let x = abs(old.maxX - area.maxX) < 3 ? old.maxX - size.width : old.minX
        let y = abs(old.minY - area.minY) < 3 ? old.minY : old.maxY - size.height
        let desired = NSRect(x: x, y: y, width: size.width, height: size.height)
        overlay.setFrame(fitted(desired, in: screen), display: true)
        saveFrame()
    }

    func position(_ corner: String) {
        guard let screen = overlay.screen ?? NSScreen.main else { return }
        let area = screen.visibleFrame.insetBy(dx: 16, dy: 16)
        let width = min(preferredSize.width, area.width)
        let height = min(preferredSize.height, area.height)
        let x = corner.contains("Right") ? area.maxX - width : area.minX
        let y = corner.hasPrefix("top") ? area.maxY - height : area.minY
        overlay.setFrame(NSRect(x: x, y: y, width: width, height: height), display: true)
        saveFrame()
    }
    func buildMenu() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        statusItem.button?.image = NSImage(systemSymbolName: "music.note.list", accessibilityDescription: "Lyrics Overlay – Einstellungen")
        statusItem.button?.toolTip = "Einstellungen öffnen · Rechtsklick für Menü"
        statusItem.button?.target = self
        statusItem.button?.action = #selector(statusClicked(_:))
        statusItem.button?.sendAction(on: [.leftMouseUp, .rightMouseUp])
        let menu = NSMenu()
        menu.delegate = self
        let title = NSMenuItem(title: "Lyrics Overlay", action: nil, keyEquivalent: ""); title.isEnabled = false; menu.addItem(title)
        menu.addItem(.separator())
        visibilityItem = NSMenuItem(title: "Overlay ausblenden", action: #selector(toggleVisibility), keyEquivalent: ""); visibilityItem.target = self; menu.addItem(visibilityItem)
        clickItem = NSMenuItem(title: "Klicks durchlassen", action: #selector(toggleClick), keyEquivalent: ""); clickItem.target = self; menu.addItem(clickItem)
        let players = NSMenu(title: "Wiedergabequelle")
        for source in PlayerPreference.allCases {
            let item = NSMenuItem(title: source.title, action: #selector(selectPlayer(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = source.rawValue
            item.isEnabled = !model.isDemo
            players.addItem(item)
        }
        let playerItem = NSMenuItem(title: "Wiedergabequelle", action: nil, keyEquivalent: "")
        playerItem.submenu = players
        menu.addItem(playerItem)
        let corners = NSMenu(title: "Position")
        for (title, key) in [("Oben links", "topLeft"), ("Oben rechts", "topRight"), ("Unten links", "bottomLeft"), ("Unten rechts", "bottomRight")] {
            let item = NSMenuItem(title: title, action: #selector(selectCorner(_:)), keyEquivalent: ""); item.representedObject = key; item.target = self; corners.addItem(item)
        }
        let positionItem = NSMenuItem(title: "Position", action: nil, keyEquivalent: ""); positionItem.submenu = corners; menu.addItem(positionItem)
        menu.addItem(.separator())
        let preferences = NSMenuItem(title: "Einstellungen …", action: #selector(showSettings), keyEquivalent: ","); preferences.target = self; menu.addItem(preferences)
        let quit = NSMenuItem(title: "Beenden", action: #selector(quitApp), keyEquivalent: "q"); quit.target = self; menu.addItem(quit)
        statusMenu = menu
    }
    func menuWillOpen(_ menu: NSMenu) {
        guard menu === statusMenu,
              let sources = menu.items.first(where: { $0.title == "Wiedergabequelle" })?.submenu else { return }
        let selected = UserDefaults.standard.string(forKey: "playerPreference") ?? PlayerPreference.automatic.rawValue
        for item in sources.items {
            item.state = (item.representedObject as? String) == selected ? .on : .off
        }
    }
    @objc func statusClicked(_ sender: NSStatusBarButton) {
        if NSApp.currentEvent?.type == .rightMouseUp {
            statusItem.menu = statusMenu
            sender.performClick(nil)
            statusItem.menu = nil
        } else {
            showSettings()
        }
    }
    @objc func toggleVisibility() { if overlay.isVisible { overlay.orderOut(nil) } else { overlay.orderFrontRegardless() }; visibilityItem.title = overlay.isVisible ? "Overlay ausblenden" : "Overlay einblenden" }
    private func setClickThrough(_ enabled: Bool) {
        clickThrough = enabled
        overlay.ignoresMouseEvents = enabled
        clickItem.state = enabled ? .on : .off
        UserDefaults.standard.set(enabled, forKey: "clickThrough")
    }
    @objc func toggleClick() { setClickThrough(!clickThrough) }
    @objc func selectPlayer(_ sender: NSMenuItem) {
        guard !model.isDemo, let raw = sender.representedObject as? String,
              let source = PlayerPreference(rawValue: raw) else { return }
        UserDefaults.standard.set(raw, forKey: "playerPreference")
        model.setPlayerPreference(source)
    }
    @objc func selectCorner(_ sender: NSMenuItem) { if let key = sender.representedObject as? String { position(key) } }
    @objc func showSettings() {
        if settings == nil {
            settings = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 530, height: 470), styleMask: [.titled, .closable], backing: .buffered, defer: false)
            settings.title = "Lyrics Overlay – Einstellungen"
            settings.isReleasedWhenClosed = false
            settings.contentView = NSHostingView(rootView: SettingsView(model: model, resize: { [weak self] in self?.resize() }))
            settings.center()
        }
        settings.makeKeyAndOrderFront(nil); NSApp.activate(ignoringOtherApps: true)
    }
    @objc func quitApp() { NSApp.terminate(nil) }
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.run()
