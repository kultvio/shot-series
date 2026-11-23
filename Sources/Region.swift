import AppKit

struct Display: Equatable {
    let id: CGDirectDisplayID
    let identifier: String
    let frame: CGRect

    @MainActor
    static var connected: [Display] {
        NSScreen.screens.compactMap { screen in
            guard let number = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber else {
                return nil
            }
            let id = CGDirectDisplayID(number.uint32Value)
            let uuid = CGDisplayCreateUUIDFromDisplayID(id)?.takeRetainedValue()
            let identifier = uuid.map { CFUUIDCreateString(nil, $0) as String } ?? String(id)
            return Display(id: id, identifier: identifier, frame: screen.frame)
        }
    }

    var bounds: CGRect { CGRect(origin: .zero, size: frame.size) }
}

struct Region: Codable, Equatable {
    static let minimumSize: CGFloat = 32

    let displayIdentifier: String
    let rect: CGRect

    func fitted(to display: Display) -> CGRect? {
        guard display.identifier == displayIdentifier,
              rect.width >= Self.minimumSize, rect.height >= Self.minimumSize,
              rect.origin.x.isFinite, rect.origin.y.isFinite,
              rect.width.isFinite, rect.height.isFinite,
              display.bounds.width >= Self.minimumSize, display.bounds.height >= Self.minimumSize else {
            return nil
        }

        let width = min(rect.width, display.bounds.width)
        let height = min(rect.height, display.bounds.height)
        return CGRect(
            x: min(max(0, rect.minX), display.bounds.width - width),
            y: min(max(0, rect.minY), display.bounds.height - height),
            width: width,
            height: height
        )
    }

    static func dragged(from start: CGPoint, to end: CGPoint, in bounds: CGRect) -> CGRect {
        CGRect(
            x: min(start.x, end.x), y: min(start.y, end.y),
            width: abs(end.x - start.x), height: abs(end.y - start.y)
        ).integral.intersection(bounds)
    }

    // ScreenCaptureKit uses a top-left origin for the source rectangle in display points.
    static func captureRect(_ rect: CGRect, displayHeight: CGFloat) -> CGRect {
        CGRect(x: rect.minX, y: displayHeight - rect.maxY, width: rect.width, height: rect.height)
    }
}

enum RegionHandle: CaseIterable {
    case bottomLeft, bottom, bottomRight, left, right, topLeft, top, topRight

    private var horizontal: CGFloat {
        switch self {
        case .bottomLeft, .left, .topLeft: -1
        case .bottomRight, .right, .topRight: 1
        default: 0
        }
    }

    private var vertical: CGFloat {
        switch self {
        case .bottomLeft, .bottom, .bottomRight: -1
        case .topLeft, .top, .topRight: 1
        default: 0
        }
    }

    var isCorner: Bool { horizontal != 0 && vertical != 0 }

    func anchor(in rect: CGRect) -> CGPoint {
        CGPoint(x: rect.midX + horizontal * rect.width / 2, y: rect.midY + vertical * rect.height / 2)
    }

    func hitFrame(for rect: CGRect, in bounds: CGRect) -> CGRect {
        let point = anchor(in: rect)
        let width: CGFloat = isCorner || horizontal != 0 ? 18 : rect.width - 18
        let height: CGFloat = isCorner || vertical != 0 ? 18 : rect.height - 18
        return CGRect(x: point.x - width / 2, y: point.y - height / 2, width: width, height: height)
            .intersection(bounds)
    }

    func resized(_ rect: CGRect, by delta: CGPoint, in bounds: CGRect) -> CGRect {
        var minX = rect.minX
        var maxX = rect.maxX
        var minY = rect.minY
        var maxY = rect.maxY
        let minimum = Region.minimumSize

        if horizontal < 0 { minX = max(bounds.minX, min(maxX - minimum, (minX + delta.x).rounded())) }
        if horizontal > 0 { maxX = min(bounds.maxX, max(minX + minimum, (maxX + delta.x).rounded())) }
        if vertical < 0 { minY = max(bounds.minY, min(maxY - minimum, (minY + delta.y).rounded())) }
        if vertical > 0 { maxY = min(bounds.maxY, max(minY + minimum, (maxY + delta.y).rounded())) }

        return CGRect(x: minX, y: minY, width: maxX - minX, height: maxY - minY)
    }

    @MainActor
    var cursor: NSCursor {
        if #available(macOS 15.0, *) {
            switch self {
            case .bottomLeft: return .frameResize(position: .bottomLeft, directions: .all)
            case .bottom: return .frameResize(position: .bottom, directions: .all)
            case .bottomRight: return .frameResize(position: .bottomRight, directions: .all)
            case .left: return .frameResize(position: .left, directions: .all)
            case .right: return .frameResize(position: .right, directions: .all)
            case .topLeft: return .frameResize(position: .topLeft, directions: .all)
            case .top: return .frameResize(position: .top, directions: .all)
            case .topRight: return .frameResize(position: .topRight, directions: .all)
            }
        }
        if isCorner { return .crosshair }
        return horizontal == 0 ? .resizeUpDown : .resizeLeftRight
    }

    func mark(in rect: CGRect) -> NSBezierPath {
        let point = anchor(in: rect)
        let path = NSBezierPath()
        let length: CGFloat = 11
        if isCorner {
            path.move(to: CGPoint(x: point.x - horizontal * length, y: point.y))
            path.line(to: point)
            path.line(to: CGPoint(x: point.x, y: point.y - vertical * length))
        } else {
            path.move(to: CGPoint(x: point.x - abs(vertical) * length, y: point.y - abs(horizontal) * length))
            path.line(to: CGPoint(x: point.x + abs(vertical) * length, y: point.y + abs(horizontal) * length))
        }
        path.lineJoinStyle = .round
        path.lineCapStyle = .round
        return path
    }
}
