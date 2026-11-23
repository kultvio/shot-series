import AppKit
import Carbon.HIToolbox

struct CaptureKey: Identifiable {
    let number: Int
    let code: UInt32

    var id: Int { number }
    var label: String { "F\(number)" }

    static let choices: [CaptureKey] = [
        kVK_F1, kVK_F2, kVK_F3, kVK_F4, kVK_F5, kVK_F6, kVK_F7, kVK_F8, kVK_F9, kVK_F10,
        kVK_F11, kVK_F12, kVK_F13, kVK_F14, kVK_F15, kVK_F16, kVK_F17, kVK_F18, kVK_F19, kVK_F20
    ].enumerated().map { CaptureKey(number: $0.offset + 1, code: UInt32($0.element)) }
}

@MainActor
final class HotKeys {
    private static let signature: OSType = 0x4B414452
    private static let escapeID: UInt32 = 100

    var onCapture: (() -> Void)?
    var onEscape: (() -> Void)?
    private var handler: EventHandlerRef?
    private var capture: EventHotKeyRef?
    private var captureID: UInt32?
    private var escape: EventHotKeyRef?
    private var pressed: Set<UInt32> = []

    init() {
        let events = [
            EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed)),
            EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyReleased))
        ]
        InstallEventHandler(GetApplicationEventTarget(), { _, event, context in
            guard let event, let context else { return OSStatus(eventNotHandledErr) }
            var hotKey = EventHotKeyID()
            let result = GetEventParameter(event, EventParamName(kEventParamDirectObject),
                                           EventParamType(typeEventHotKeyID), nil,
                                           MemoryLayout<EventHotKeyID>.size, nil, &hotKey)
            guard result == noErr else { return result }
            let keys = Unmanaged<HotKeys>.fromOpaque(context).takeUnretainedValue()
            let isPress = GetEventKind(event) == UInt32(kEventHotKeyPressed)
            return MainActor.assumeIsolated { keys.receive(hotKey, isPress: isPress) }
        }, events.count, events, Unmanaged.passUnretained(self).toOpaque(), &handler)
    }

    func bindCapture(_ key: CaptureKey) -> Bool {
        let id = UInt32(key.number)
        if captureID == id { return true }
        guard let next = register(code: key.code, id: id) else { return false }
        if let capture { UnregisterEventHotKey(capture) }
        if let captureID { pressed.remove(captureID) }
        capture = next
        captureID = id
        return true
    }

    func bindEscape() {
        if escape == nil { escape = register(code: UInt32(kVK_Escape), id: Self.escapeID) }
    }

    func unbindEscape() {
        if let escape { UnregisterEventHotKey(escape) }
        escape = nil
        pressed.remove(Self.escapeID)
    }

    func stop() {
        unbindEscape()
        if let capture { UnregisterEventHotKey(capture) }
        if let handler { RemoveEventHandler(handler) }
    }

    private func register(code: UInt32, id: UInt32) -> EventHotKeyRef? {
        guard handler != nil else { return nil }
        var reference: EventHotKeyRef?
        let result = RegisterEventHotKey(code, 0, EventHotKeyID(signature: Self.signature, id: id),
                                         GetApplicationEventTarget(), UInt32(kEventHotKeyExclusive), &reference)
        return result == noErr ? reference : nil
    }

    private func receive(_ key: EventHotKeyID, isPress: Bool) -> OSStatus {
        guard key.signature == Self.signature, key.id == captureID || key.id == Self.escapeID else {
            return OSStatus(eventNotHandledErr)
        }
        if isPress {
            if pressed.insert(key.id).inserted {
                if key.id == Self.escapeID { onEscape?() } else { onCapture?() }
            }
        } else {
            pressed.remove(key.id)
        }
        return noErr
    }
}
