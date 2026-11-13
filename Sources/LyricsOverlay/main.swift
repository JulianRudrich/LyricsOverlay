import AppKit
import Carbon
import SwiftUI
import LyricsCore

struct Track: Equatable {
    let id: String
    let title: String
    let artist: String
    let duration: Double
}

final class PlayerModel: ObservableObject {
    @Published var track: Track?
    @Published var lines: [LyricLine] = []
    @Published var position: Double = 0
    @Published var status = "Warte auf Spotify …"
    @Published var playing = false
    private var generation = 0
    private var pending = false
    private let queue = DispatchQueue(label: "spotify-reader")
    private var timer: Timer?

    func start() {
        poll()
        timer = Timer.scheduledTimer(withTimeInterval: 0.8, repeats: true) { [weak self] _ in self?.poll() }
    }

    private func poll() {
        guard !pending else { return }
        pending = true
        queue.async { [weak self] in
            let result = Self.readSpotify()
            DispatchQueue.main.async {
                guard let self else { return }
                self.pending = false
                switch result {
                case .failure(let error): self.clear(error.localizedDescription)
                case .success(let snapshot): self.update(snapshot)
                }
            }
        }
    }

    private struct Snapshot { let track: Track; let position: Double; let playing: Bool }
    private struct ReadError: LocalizedError { let message: String; var errorDescription: String? { message } }

    private static func readSpotify() -> Result<Snapshot, ReadError> {
        guard NSRunningApplication.runningApplications(withBundleIdentifier: "com.spotify.client").first != nil else {
            return .failure(ReadError(message: "Öffne Spotify und spiele einen Song ab."))
        }
        let source = """
        tell application id "com.spotify.client"
            if player state is stopped then return {"stopped"}
            set t to current track
            return {id of t, name of t, artist of t, duration of t, player position, (player state is playing)}
        end tell
        """
        var error: NSDictionary?
        guard let descriptor = NSAppleScript(source: source)?.executeAndReturnError(&error) else {
            let number = error?[NSAppleScript.errorNumber] as? Int
            let detail = error?[NSAppleScript.errorMessage] as? String ?? "Spotify konnte nicht gelesen werden."
            return .failure(ReadError(message: number == -1743 ? "Spotify-Zugriff erlauben: Systemeinstellungen → Datenschutz & Sicherheit → Automation." : "Spotify: \(detail)"))
        }
        guard descriptor.numberOfItems >= 6,
              let id = descriptor.atIndex(1)?.stringValue,
              let title = descriptor.atIndex(2)?.stringValue,
              let artist = descriptor.atIndex(3)?.stringValue,
              let duration = descriptor.atIndex(4), let position = descriptor.atIndex(5) else {
            return .failure(ReadError(message: "Spiele einen Song in Spotify ab."))
        }
        let track = Track(id: id, title: title, artist: artist, duration: duration.doubleValue / 1000)
        return .success(Snapshot(track: track, position: position.doubleValue, playing: descriptor.atIndex(6)?.booleanValue ?? false))
    }

    private func clear(_ message: String) {
        if track != nil { generation += 1 }
        track = nil; lines = []; position = 0; playing = false; status = message
    }

    private func update(_ snapshot: Snapshot) {
        position = snapshot.position
        playing = snapshot.playing
        guard track != snapshot.track else { return }
        track = snapshot.track
        lines = []
        status = "Suche synchronisierte Lyrics …"
        generation += 1
        let requestedGeneration = generation
        let track = snapshot.track
        var components = URLComponents(string: "https://lrclib.net/api/search")!
        components.queryItems = [URLQueryItem(name: "track_name", value: track.title), URLQueryItem(name: "artist_name", value: track.artist)]
        var request = URLRequest(url: components.url!)
        request.timeoutInterval = 15
        request.setValue("LyricsOverlay/1.0 (macOS personal prototype)", forHTTPHeaderField: "User-Agent")
        URLSession.shared.dataTask(with: request) { [weak self] data, response, error in
            let httpOK = (response as? HTTPURLResponse).map { (200..<300).contains($0.statusCode) } ?? false
            let results = data.flatMap { try? JSONDecoder().decode([LyricsResult].self, from: $0) }
            let match = results.flatMap { LyricsMatching.best($0, title: track.title, artist: track.artist, duration: track.duration) }
            let parsed = match?.syncedLyrics.map(LRC.parse) ?? []
            DispatchQueue.main.async {
                guard let self, self.generation == requestedGeneration else { return }
                self.lines = parsed
                self.status = error != nil || !httpOK || results == nil ? "Lyrics konnten nicht geladen werden. Prüfe deine Internetverbindung." : "Keine synchronisierten Lyrics für diesen Song gefunden."
            }
        }.resume()
    }
}

struct OverlayView: View {
    @ObservedObject var model: PlayerModel
    var showSettings: () -> Void
    @AppStorage("fontSize") private var fontSize = 25.0
    @AppStorage("opacity") private var opacity = 0.78
    @AppStorage("overlayOpacity") private var overlayOpacity = 1.0
    @AppStorage("offset") private var offset = 0.0

    private var active: Int? { LRC.currentIndex(model.lines, at: model.position + offset) }
    private var visibleLines: [(Int, LyricLine)] {
        guard !model.lines.isEmpty else { return [] }
        let start = max(0, (active ?? 0) - 1)
        return Array(model.lines.enumerated().dropFirst(start).prefix(active == nil || active == 0 ? 3 : 4))
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 6) {
                Button(action: showSettings) {
                    Image(systemName: "music.note")
                        .frame(width: 28, height: 28)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help("Einstellungen öffnen")
                Text(model.track.map { "\($0.title)  ·  \($0.artist)" } ?? "LYRICS OVERLAY")
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
            .font(.system(size: 11, weight: .semibold)).foregroundStyle(.white.opacity(0.62))
            if model.lines.isEmpty {
                Text(model.status).font(.system(size: 15, weight: .medium)).foregroundStyle(.white.opacity(0.8))
            } else {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(visibleLines, id: \.0) { index, line in
                        Text(line.text.isEmpty ? "♪" : line.text)
                            .font(.system(size: index == active ? fontSize : fontSize * 0.8, weight: index == active ? .bold : .medium))
                            .foregroundStyle(.white.opacity(index == active ? 1 : 0.4))
                            .lineLimit(2)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                .animation(.easeInOut(duration: 0.2), value: active)
            }
            Spacer(minLength: 0)
        }
        .padding(18)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(Color.black.opacity(opacity), in: RoundedRectangle(cornerRadius: 16))
        .opacity(overlayOpacity)
    }
}

struct SettingsView: View {
    @AppStorage("fontSize") private var fontSize = 25.0
    @AppStorage("opacity") private var opacity = 0.78
    @AppStorage("overlayOpacity") private var overlayOpacity = 1.0
    @AppStorage("offset") private var offset = 0.0
    @AppStorage("overlayWidth") private var width = 400.0
    @AppStorage("overlayHeight") private var height = 240.0
    var resize: () -> Void
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text("Lyrics Overlay").font(.title2.bold())
            control("Schriftgröße", value: $fontSize, range: 14...44, suffix: " pt")
            control("Transparenz: gesamtes Overlay", value: $overlayOpacity, range: 0.15...1, multiplier: 100, suffix: " % Deckkraft")
            control("Transparenz: Hintergrund", value: $opacity, range: 0...1, multiplier: 100, suffix: " % Deckkraft")
            control("Breite", value: $width, range: 260...800, suffix: " px")
            control("Höhe", value: $height, range: 160...500, suffix: " px")
            control("Timing-Korrektur", value: $offset, range: -5...5, suffix: " s", decimals: 1)
            Text("Overlay ein-/ausblenden: ⌥ ⇧ L\nPositive Werte zeigen die Lyrics früher. Songtitel und Interpret werden zur Suche an LRCLIB gesendet.")
                .font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
        }
        .padding(24).frame(width: 420)
        .onChange(of: width) { _ in resize() }
        .onChange(of: height) { _ in resize() }
    }
    private func control(_ title: String, value: Binding<Double>, range: ClosedRange<Double>, multiplier: Double = 1, suffix: String, decimals: Int = 0) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack { Text(title); Spacer(); Text(String(format: "%.*f", decimals, value.wrappedValue * multiplier) + suffix).monospacedDigit().foregroundStyle(.secondary) }
            Slider(value: value, in: range)
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    let model = PlayerModel()
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
        UserDefaults.standard.register(defaults: ["fontSize": 25.0, "opacity": 0.78, "overlayOpacity": 1.0, "offset": 0.0, "overlayWidth": 400.0, "overlayHeight": 240.0])
        NSApp.setActivationPolicy(.accessory)
        overlay = NSPanel(contentRect: NSRect(x: 0, y: 0, width: UserDefaults.standard.double(forKey: "overlayWidth"), height: UserDefaults.standard.double(forKey: "overlayHeight")), styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        overlay.isOpaque = false; overlay.backgroundColor = .clear; overlay.hasShadow = true
        overlay.level = .floating; overlay.isMovableByWindowBackground = true
        overlay.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        overlay.contentView = NSHostingView(rootView: OverlayView(model: model, showSettings: { [weak self] in self?.showSettings() }))
        overlay.delegate = self
        if let saved = UserDefaults.standard.string(forKey: "windowFrame"), NSRectFromString(saved).width > 0 {
            let rect = NSRectFromString(saved)
            if NSScreen.screens.contains(where: { $0.visibleFrame.intersects(rect) }) { overlay.setFrame(rect, display: true) }
            else { position("topRight") }
        } else { position("topRight") }
        overlay.orderFrontRegardless()
        buildMenu()
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
        clickThrough = false
        overlay.ignoresMouseEvents = false
        clickItem.state = .off
        overlay.orderFrontRegardless()
        visibilityItem.title = "Overlay ausblenden"
        showSettings()
        return true
    }
    func windowDidMove(_ notification: Notification) { saveFrame() }
    private func saveFrame() { if overlay != nil { UserDefaults.standard.set(NSStringFromRect(overlay.frame), forKey: "windowFrame") } }
    func resize() {
        guard overlay != nil else { return }
        var frame = overlay.frame
        let newHeight = UserDefaults.standard.double(forKey: "overlayHeight")
        frame.origin.y += frame.height - newHeight
        frame.size = NSSize(width: UserDefaults.standard.double(forKey: "overlayWidth"), height: newHeight)
        overlay.setFrame(frame, display: true); saveFrame()
    }
    func position(_ corner: String) {
        let screen = overlay.screen ?? NSScreen.main ?? NSScreen.screens.first!
        let area = screen.visibleFrame.insetBy(dx: 16, dy: 16)
        let size = overlay.frame.size
        let x = corner.contains("Right") ? area.maxX - size.width : area.minX
        let y = corner.hasPrefix("top") ? area.maxY - size.height : area.minY
        overlay.setFrameOrigin(NSPoint(x: x, y: y)); saveFrame()
    }
    func buildMenu() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        statusItem.button?.image = NSImage(systemSymbolName: "music.note.list", accessibilityDescription: "Lyrics Overlay – Einstellungen")
        statusItem.button?.toolTip = "Einstellungen öffnen · Rechtsklick für Menü"
        statusItem.button?.target = self
        statusItem.button?.action = #selector(statusClicked(_:))
        statusItem.button?.sendAction(on: [.leftMouseUp, .rightMouseUp])
        let menu = NSMenu()
        let title = NSMenuItem(title: "Lyrics Overlay", action: nil, keyEquivalent: ""); title.isEnabled = false; menu.addItem(title)
        menu.addItem(.separator())
        visibilityItem = NSMenuItem(title: "Overlay ausblenden", action: #selector(toggleVisibility), keyEquivalent: ""); visibilityItem.target = self; menu.addItem(visibilityItem)
        clickItem = NSMenuItem(title: "Klicks durchlassen", action: #selector(toggleClick), keyEquivalent: ""); clickItem.target = self; menu.addItem(clickItem)
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
    @objc func toggleClick() { clickThrough.toggle(); overlay.ignoresMouseEvents = clickThrough; clickItem.state = clickThrough ? .on : .off }
    @objc func selectCorner(_ sender: NSMenuItem) { if let key = sender.representedObject as? String { position(key) } }
    @objc func showSettings() {
        if settings == nil {
            settings = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 420, height: 510), styleMask: [.titled, .closable], backing: .buffered, defer: false)
            settings.title = "Lyrics Overlay – Einstellungen"
            settings.isReleasedWhenClosed = false
            settings.contentView = NSHostingView(rootView: SettingsView(resize: { [weak self] in self?.resize() }))
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
