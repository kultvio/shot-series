import CoreGraphics
import Foundation
import ImageIO
import ScreenCaptureKit
import UniformTypeIdentifiers

struct CaptureRequest: Sendable {
    let displayID: CGDirectDisplayID
    let sourceRect: CGRect
    let directory: URL
}

enum CaptureError: LocalizedError {
    case missingDisplay, encodingFailed, numberExhausted

    var errorDescription: String? {
        switch self {
        case .missingDisplay: "Экран с выделенной областью недоступен."
        case .encodingFailed: "Не удалось создать PNG."
        case .numberExhausted: "Не удалось назначить следующий номер файла. Создайте новую серию."
        }
    }
}

actor CaptureService {
    func save(_ request: CaptureRequest) async throws -> URL {
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
        guard let display = content.displays.first(where: { $0.displayID == request.displayID }) else {
            throw CaptureError.missingDisplay
        }

        let pid = ProcessInfo.processInfo.processIdentifier
        let ownApps = content.applications.filter { $0.processID == pid }
        let filter: SCContentFilter
        if ownApps.isEmpty {
            filter = SCContentFilter(display: display, excludingWindows: content.windows.filter {
                $0.owningApplication?.processID == pid
            })
        } else {
            filter = SCContentFilter(display: display, excludingApplications: ownApps, exceptingWindows: [])
        }
        if #available(macOS 14.2, *) { filter.includeMenuBar = true }

        let configuration = SCStreamConfiguration()
        let scale = CGFloat(filter.pointPixelScale)
        configuration.sourceRect = request.sourceRect
        configuration.width = max(1, Int((request.sourceRect.width * scale).rounded()))
        configuration.height = max(1, Int((request.sourceRect.height * scale).rounded()))
        configuration.showsCursor = false

        let image = try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: configuration)
        let png = NSMutableData()
        guard let encoder = CGImageDestinationCreateWithData(png, UTType.png.identifier as CFString, 1, nil) else {
            throw CaptureError.encodingFailed
        }
        CGImageDestinationAddImage(encoder, image, nil)
        guard CGImageDestinationFinalize(encoder) else { throw CaptureError.encodingFailed }
        return try ScreenshotFiles.write(png as Data, in: request.directory)
    }
}
