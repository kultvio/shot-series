import AppKit
import Combine
import SwiftUI

struct AppMessage {
    let text: String
    var isError = false
}

@MainActor
final class AppController: NSObject, NSApplicationDelegate, ObservableObject {
    @Published private(set) var root: URL?
    @Published private(set) var series: [URL] = []
    @Published private(set) var activeSeries: URL?
    @Published private(set) var key = CaptureKey.choices[7]
    @Published private(set) var regionSize: CGSize?
    @Published private(set) var drawing = false
    @Published private(set) var overlayVisible = true
    @Published private(set) var needsScreenAccess = false
    @Published private(set) var message: AppMessage?

    private let preferences = UserDefaults.standard
    private let overlay = SelectionOverlay()
    private let hotKeys = HotKeys()
    private let captureService = CaptureService()
    private let popover = NSPopover()
    private var statusItem: NSStatusItem?
    private var region: Region?
    private var requests: [CaptureRequest] = []
    private var captureTask: Task<Void, Never>?
    private var iconReset: Task<Void, Never>?
    private var openingFolder = false

    func applicationDidFinishLaunching(_ notification: Notification) {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        setStatusIcon("viewfinder")
        statusItem?.button?.target = self
        statusItem?.button?.action = #selector(togglePopover)

        popover.behavior = .transient
        let hosting = NSHostingController(rootView: Controls(app: self))
        hosting.sizingOptions = [.preferredContentSize]
        popover.contentViewController = hosting

        root = preferences.string(forKey: "rootPath").map { URL(fileURLWithPath: $0, isDirectory: true) }
        if let root, let name = preferences.string(forKey: "seriesName") {
            activeSeries = root.appendingPathComponent(name, isDirectory: true)
        }
        if let data = preferences.data(forKey: "region") { region = try? JSONDecoder().decode(Region.self, from: data) }
        overlayVisible = preferences.object(forKey: "overlayVisible") as? Bool ?? true
        if let savedKey = CaptureKey.choices.first(where: { $0.number == preferences.integer(forKey: "captureKey") }) {
            key = savedKey
        }

        hotKeys.onCapture = { [weak self] in self?.capture() }
        hotKeys.onEscape = { [weak self] in self?.cancelDrawing() }
        if !hotKeys.bindCapture(key) { report("Клавиша \(key.label) занята. Выберите другую.", error: true) }
        overlay.onCancel = { [weak self] in self?.cancelDrawing() }
        overlay.onPick = { [weak self] region in self?.finishDrawing(region) }
        overlay.onResize = { [weak self] region, finished in
            self?.region = region
            self?.regionSize = region.rect.size
            if finished { self?.storeRegion() }
        }

        NotificationCenter.default.addObserver(self, selector: #selector(displaysChanged),
                                               name: NSApplication.didChangeScreenParametersNotification, object: nil)
        NSWorkspace.shared.notificationCenter.addObserver(self, selector: #selector(spaceChanged),
                                                           name: NSWorkspace.activeSpaceDidChangeNotification, object: nil)
        refreshSeries()
        showSavedRegion()
        if root == nil || region == nil { showPopover() }
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        showPopover()
        return false
    }

    func applicationWillTerminate(_ notification: Notification) {
        hotKeys.stop()
    }

    @objc private func togglePopover() {
        if popover.isShown { popover.performClose(nil) } else { showPopover() }
    }

    private func showPopover() {
        guard !openingFolder, !popover.isShown, let button = statusItem?.button, button.window != nil else { return }
        refreshSeries()
        if needsScreenAccess && CGPreflightScreenCaptureAccess() {
            needsScreenAccess = false
            message = nil
        }
        popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
    }

    func chooseRoot() {
        guard !openingFolder else { return }
        openingFolder = true
        popover.performClose(nil)
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.directoryURL = root
        panel.prompt = "Выбрать"
        panel.message = "Выберите каталог для папок со снимками."
        NSApp.activate()
        panel.begin { [weak self] result in
            guard let self else { return }
            openingFolder = false
            if result == .OK, let selected = panel.url?.standardizedFileURL {
                if selected != root {
                    root = selected
                    activeSeries = nil
                    preferences.set(selected.path, forKey: "rootPath")
                    preferences.removeObject(forKey: "seriesName")
                }
                message = nil
            }
            showPopover()
        }
    }

    func addSeries() {
        guard let root else { return }
        do {
            let folder = try ScreenshotFiles.createSeries(in: root)
            chooseSeries(folder)
            refreshSeries()
        } catch {
            report("Не удалось создать папку. \(error.localizedDescription)", error: true)
        }
    }

    func chooseSeries(_ folder: URL) {
        activeSeries = folder
        preferences.set(folder.lastPathComponent, forKey: "seriesName")
        if !needsScreenAccess { message = nil }
    }

    private func refreshSeries() {
        guard let root else { return }
        do {
            series = try ScreenshotFiles.subfolders(in: root)
            if let activeSeries, !series.contains(where: { $0.path == activeSeries.path }) {
                self.activeSeries = nil
                preferences.removeObject(forKey: "seriesName")
            }
        } catch {
            series = []
            report("Каталог недоступен. \(error.localizedDescription)", error: true)
        }
    }

    func revealFolder() {
        guard let folder = activeSeries ?? root else { return }
        popover.performClose(nil)
        NSWorkspace.shared.open(folder)
    }

    func chooseKey(_ key: CaptureKey) {
        guard hotKeys.bindCapture(key) else {
            report("Клавиша \(key.label) занята. Выберите другую.", error: true)
            return
        }
        self.key = key
        preferences.set(key.number, forKey: "captureKey")
        if !needsScreenAccess { message = nil }
    }

    func beginDrawing() {
        if drawing {
            cancelDrawing()
            return
        }
        let displays = Display.connected
        guard !displays.isEmpty else { return }
        popover.performClose(nil)
        drawing = true
        hotKeys.bindEscape()
        overlay.begin(on: displays)
    }

    private func finishDrawing(_ region: Region) {
        self.region = region
        setOverlayVisible(true)
        storeRegion()
        if !needsScreenAccess { message = nil }
        cancelDrawing()
    }

    private func cancelDrawing() {
        guard drawing else { return }
        drawing = false
        hotKeys.unbindEscape()
        showSavedRegion()
    }

    func toggleOverlay() {
        popover.performClose(nil)
        setOverlayVisible(!overlayVisible)
        showSavedRegion()
    }

    private func setOverlayVisible(_ visible: Bool) {
        overlayVisible = visible
        preferences.set(visible, forKey: "overlayVisible")
    }

    private func storeRegion() {
        if let region, let data = try? JSONEncoder().encode(region) { preferences.set(data, forKey: "region") }
    }

    private func availableRegion() -> (Display, CGRect)? {
        guard let region, let display = Display.connected.first(where: { $0.identifier == region.displayIdentifier }),
              let rect = region.fitted(to: display) else { return nil }
        return (display, rect)
    }

    private func showSavedRegion() {
        let available = availableRegion()
        regionSize = available?.1.size
        if overlayVisible, let (display, rect) = available { overlay.show(rect, on: display) } else { overlay.hide() }
    }

    @objc private func displaysChanged() {
        if drawing { cancelDrawing() } else { showSavedRegion() }
    }

    @objc private func spaceChanged() { overlay.restoreOrder() }

    func capture() {
        guard !drawing else { return }
        guard let folder = activeSeries else {
            report(root == nil ? "Выберите каталог для снимков." : "Создайте серию кнопкой «+» или выберите папку.")
            showPopover()
            return
        }
        guard let (display, rect) = availableRegion() else {
            beginDrawing()
            return
        }
        guard CGPreflightScreenCaptureAccess() else {
            if !needsScreenAccess { _ = CGRequestScreenCaptureAccess() }
            needsScreenAccess = true
            report("Разрешите запись экрана в Системных настройках и снова нажмите \(key.label).", error: true)
            showPopover()
            return
        }
        needsScreenAccess = false
        popover.performClose(nil)
        requests.append(CaptureRequest(displayID: display.id,
                                       sourceRect: Region.captureRect(rect, displayHeight: display.bounds.height),
                                       directory: folder))
        if captureTask == nil {
            captureTask = Task { await processCaptures() }
        }
    }

    private func processCaptures() async {
        while !requests.isEmpty {
            let request = requests.removeFirst()
            do {
                let file = try await captureService.save(request)
                report("Сохранено: \(file.deletingLastPathComponent().lastPathComponent) / \(file.lastPathComponent)",
                       icon: "checkmark.circle")
            } catch {
                if !CGPreflightScreenCaptureAccess() {
                    needsScreenAccess = true
                    report("macOS не дала доступ к экрану. Откройте Системные настройки.", error: true)
                } else {
                    report("Не удалось сохранить снимок. \(error.localizedDescription)", error: true)
                }
            }
        }
        captureTask = nil
    }

    func openScreenSettings() {
        if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture") {
            popover.performClose(nil)
            NSWorkspace.shared.open(url)
        }
    }

    private func report(_ text: String, error: Bool = false, icon: String? = nil) {
        message = AppMessage(text: text, isError: error)
        guard let symbol = error ? "exclamationmark.circle" : icon else { return }
        iconReset?.cancel()
        setStatusIcon(symbol, help: text)
        iconReset = Task { [weak self] in
            try? await Task.sleep(for: .seconds(2))
            if !Task.isCancelled { self?.setStatusIcon("viewfinder", help: text) }
        }
    }

    private func setStatusIcon(_ symbol: String, help: String? = nil) {
        let image = NSImage(systemSymbolName: symbol, accessibilityDescription: "ShotSeries")
        image?.isTemplate = true
        statusItem?.button?.image = image
        statusItem?.button?.toolTip = help ?? "ShotSeries"
    }
}
