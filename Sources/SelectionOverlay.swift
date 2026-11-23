import AppKit

@MainActor
private final class OverlayPanel: NSPanel {
    init(frame: CGRect, interactive: Bool, aboveShade: Bool = false) {
        super.init(contentRect: frame, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        isOpaque = false
        hasShadow = false
        backgroundColor = .clear
        hidesOnDeactivate = false
        isMovable = false
        ignoresMouseEvents = !interactive
        level = NSWindow.Level(rawValue: NSWindow.Level.floating.rawValue + (aboveShade ? 1 : 0))
        collectionBehavior = [.canJoinAllSpaces, .canJoinAllApplications, .fullScreenAuxiliary, .ignoresCycle]
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

@MainActor
private class ShadeView: NSView {
    var region: CGRect? { didSet { needsDisplay = true } }

    override func draw(_ dirtyRect: NSRect) {
        let shade = NSBezierPath(rect: bounds)
        if let region { shade.appendRect(region) }
        shade.windingRule = .evenOdd
        NSColor.black.withAlphaComponent(0.27).setFill()
        shade.fill()

        guard let region else { return }
        let outline = NSBezierPath(rect: region)
        NSColor.black.withAlphaComponent(0.62).setStroke()
        outline.lineWidth = 3
        outline.stroke()
        NSColor.white.withAlphaComponent(0.9).setStroke()
        outline.lineWidth = 1
        outline.stroke()

        guard region.width >= Region.minimumSize, region.height >= Region.minimumSize else { return }
        for handle in RegionHandle.allCases {
            let mark = handle.mark(in: region)
            NSColor(white: 0.08, alpha: 0.85).setStroke()
            mark.lineWidth = 6
            mark.stroke()
            NSColor(srgbRed: 0.86, green: 0.82, blue: 1, alpha: 1).setStroke()
            mark.lineWidth = 3.5
            mark.stroke()
        }
    }
}

@MainActor
private final class DrawingView: ShadeView {
    let display: Display
    var onSelection: ((Display, CGRect) -> Void)?
    var onCancel: (() -> Void)?
    private var start: CGPoint?

    init(display: Display) {
        self.display = display
        super.init(frame: display.bounds)
        setAccessibilityLabel("Выделение области снимка")
    }

    required init?(coder: NSCoder) { nil }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func resetCursorRects() {
        addCursorRect(bounds, cursor: .crosshair)
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        if let region, !region.isEmpty {
            drawLabel("\(Int(region.width)) × \(Int(region.height))", x: region.midX, y: max(8, region.minY - 35))
        } else {
            drawLabel("Потяните мышью, чтобы выделить область   ·   Esc — отменить", x: bounds.midX, y: bounds.midY)
        }
    }

    private func drawLabel(_ label: String, x: CGFloat, y: CGFloat) {
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.monospacedDigitSystemFont(ofSize: 13, weight: .medium),
            .foregroundColor: NSColor.white
        ]
        let size = label.size(withAttributes: attributes)
        let width = size.width + 24
        let origin = CGPoint(x: max(6, min(x - width / 2, bounds.width - width - 6)), y: y)
        let box = CGRect(origin: origin, size: CGSize(width: width, height: size.height + 12))
        NSColor(white: 0.09, alpha: 0.94).setFill()
        NSBezierPath(roundedRect: box, xRadius: 8, yRadius: 8).fill()
        label.draw(at: CGPoint(x: origin.x + 12, y: origin.y + 6), withAttributes: attributes)
    }

    override func mouseDown(with event: NSEvent) {
        start = convert(event.locationInWindow, from: nil)
    }

    override func mouseDragged(with event: NSEvent) {
        guard let start else { return }
        region = Region.dragged(from: start, to: convert(event.locationInWindow, from: nil), in: bounds)
    }

    override func mouseUp(with event: NSEvent) {
        guard let start else { return }
        self.start = nil
        let rect = Region.dragged(from: start, to: convert(event.locationInWindow, from: nil), in: bounds)
        guard rect.width >= Region.minimumSize, rect.height >= Region.minimumSize else {
            region = nil
            return
        }
        onSelection?(display, rect)
    }

    override func rightMouseDown(with event: NSEvent) { onCancel?() }
}

@MainActor
private final class HandleView: NSView {
    let handle: RegionHandle
    var onDrag: ((RegionHandle, CGPoint, CGPoint, Bool) -> Void)?
    private var start: CGPoint?

    init(handle: RegionHandle) {
        self.handle = handle
        super.init(frame: .zero)
    }

    required init?(coder: NSCoder) { nil }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func resetCursorRects() { addCursorRect(bounds, cursor: handle.cursor) }

    override func draw(_ dirtyRect: CGRect) {
        // Give the clear window a hit area even where the visible grip is thin.
        NSColor(white: 0.1, alpha: 0.025).setFill()
        dirtyRect.fill()
    }

    override func mouseDown(with event: NSEvent) {
        let point = screenPoint(for: event)
        start = point
        onDrag?(handle, point, point, false)
    }

    override func mouseDragged(with event: NSEvent) {
        guard let start else { return }
        onDrag?(handle, start, screenPoint(for: event), false)
    }

    override func mouseUp(with event: NSEvent) {
        guard let start else { return }
        self.start = nil
        onDrag?(handle, start, screenPoint(for: event), true)
    }

    private func screenPoint(for event: NSEvent) -> CGPoint {
        window?.convertPoint(toScreen: event.locationInWindow) ?? NSEvent.mouseLocation
    }
}

@MainActor
final class SelectionOverlay {
    var onPick: ((Region) -> Void)?
    var onResize: ((Region, Bool) -> Void)?
    var onCancel: (() -> Void)?

    private var drawingPanels: [OverlayPanel] = []
    private var shadePanel: OverlayPanel?
    private var handles: [(RegionHandle, OverlayPanel)] = []
    private var resizeStart: CGRect?

    func begin(on displays: [Display]) {
        hide()
        for display in displays {
            let view = DrawingView(display: display)
            view.onSelection = { [weak self] display, rect in
                self?.onPick?(Region(displayIdentifier: display.identifier, rect: rect))
            }
            view.onCancel = { [weak self] in self?.onCancel?() }
            let panel = OverlayPanel(frame: display.frame, interactive: true)
            panel.contentView = view
            drawingPanels.append(panel)
            panel.orderFrontRegardless()
        }
    }

    func show(_ rect: CGRect, on display: Display) {
        hide()
        let shade = ShadeView(frame: display.bounds)
        shade.region = rect
        let panel = OverlayPanel(frame: display.frame, interactive: false)
        panel.contentView = shade
        shadePanel = panel
        panel.orderFrontRegardless()

        for handle in RegionHandle.allCases {
            let view = HandleView(handle: handle)
            view.onDrag = { [weak self] handle, start, point, finished in
                self?.resize(handle: handle, from: start, to: point, display: display, finished: finished)
            }
            let grip = OverlayPanel(frame: .zero, interactive: true, aboveShade: true)
            grip.contentView = view
            handles.append((handle, grip))
        }
        update(rect, on: display)
        for (_, grip) in handles { grip.orderFrontRegardless() }
    }

    func restoreOrder() {
        for panel in drawingPanels { panel.orderFrontRegardless() }
        shadePanel?.orderFrontRegardless()
        for (_, grip) in handles { grip.orderFrontRegardless() }
    }

    func hide() {
        for panel in drawingPanels { panel.orderOut(nil) }
        for (_, panel) in handles { panel.orderOut(nil) }
        shadePanel?.orderOut(nil)
        drawingPanels.removeAll()
        handles.removeAll()
        shadePanel = nil
        resizeStart = nil
    }

    private func update(_ rect: CGRect, on display: Display) {
        (shadePanel?.contentView as? ShadeView)?.region = rect
        for (handle, grip) in handles {
            let localFrame = handle.hitFrame(for: rect, in: display.bounds)
            grip.setFrame(localFrame.offsetBy(dx: display.frame.minX, dy: display.frame.minY), display: true)
        }
    }

    private func resize(handle: RegionHandle, from start: CGPoint, to point: CGPoint, display: Display, finished: Bool) {
        guard let shade = shadePanel?.contentView as? ShadeView, let current = shade.region else { return }
        if resizeStart == nil { resizeStart = current }
        guard let resizeStart else { return }
        let delta = CGPoint(x: point.x - start.x, y: point.y - start.y)
        let rect = handle.resized(resizeStart, by: delta, in: display.bounds)
        update(rect, on: display)
        onResize?(Region(displayIdentifier: display.identifier, rect: rect), finished)
        if finished { self.resizeStart = nil }
    }
}
