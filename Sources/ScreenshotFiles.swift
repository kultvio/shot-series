import Darwin
import Foundation

enum ScreenshotFiles {
    static func subfolders(in root: URL) throws -> [URL] {
        let keys: Set<URLResourceKey> = [.isDirectoryKey, .isSymbolicLinkKey]
        return try FileManager.default.contentsOfDirectory(at: root, includingPropertiesForKeys: Array(keys),
                                                           options: .skipsHiddenFiles)
            .filter {
                let values = try? $0.resourceValues(forKeys: keys)
                return values?.isDirectory == true && values?.isSymbolicLink != true
            }
            .sorted { $0.lastPathComponent.localizedStandardCompare($1.lastPathComponent) == .orderedAscending }
    }

    static func createSeries(in root: URL) throws -> URL {
        let files = FileManager.default
        let entries = try files.contentsOfDirectory(atPath: root.path)
        var number = try nextNumber(after: entries.compactMap { name in
            let prefix = "Серия "
            guard name.hasPrefix(prefix) else { return nil }
            return positiveNumber(name.dropFirst(prefix.count))
        }.max() ?? 0)

        while true {
            let folder = root.appendingPathComponent("Серия \(number)", isDirectory: true)
            // mkdir is exclusive even if another process is creating series in the same directory.
            let error = folder.withUnsafeFileSystemRepresentation { path -> Int32 in
                guard let path else { return EINVAL }
                return mkdir(path, 0o777) == 0 ? 0 : errno
            }
            if error == 0 { return folder }
            guard error == EEXIST else { throw POSIXError(POSIXErrorCode(rawValue: error) ?? .EIO) }
            number = try nextNumber(after: number)
        }
    }

    static func write(_ png: Data, in directory: URL) throws -> URL {
        let files = FileManager.default
        let entries = try files.contentsOfDirectory(atPath: directory.path)
        var number = try nextNumber(after: entries.compactMap { name in
            guard name.lowercased().hasSuffix(".png") else { return nil }
            return positiveNumber(name.dropLast(4))
        }.max() ?? 0)

        let staging = directory.appendingPathComponent(".shot-series-\(UUID().uuidString).tmp")
        try png.write(to: staging, options: .atomic)
        defer { try? files.removeItem(at: staging) }

        while true {
            let destination = directory.appendingPathComponent("\(number).png")
            let error = staging.withUnsafeFileSystemRepresentation { source in
                destination.withUnsafeFileSystemRepresentation { target -> Int32 in
                    guard let source, let target else { return EINVAL }
                    return renamex_np(source, target, UInt32(RENAME_EXCL)) == 0 ? 0 : errno
                }
            }
            switch error {
            case 0:
                return destination
            case ENOTSUP, EINVAL:
                // Some external filesystems do not implement exclusive rename.
                do {
                    try png.write(to: destination, options: .withoutOverwriting)
                    return destination
                } catch CocoaError.fileWriteFileExists {
                    number = try nextNumber(after: number)
                }
            case EEXIST, EISDIR:
                number = try nextNumber(after: number)
            default:
                throw POSIXError(POSIXErrorCode(rawValue: error) ?? .EIO)
            }
        }
    }

    private static func positiveNumber(_ digits: Substring) -> Int? {
        guard !digits.isEmpty, digits.utf8.allSatisfy({ (48...57).contains($0) }),
              let number = Int(digits), number > 0 else { return nil }
        return number
    }

    private static func nextNumber(after number: Int) throws -> Int {
        guard number < Int.max else { throw CaptureError.numberExhausted }
        return number + 1
    }
}
