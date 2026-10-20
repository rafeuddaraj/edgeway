import AppKit

/// One active display in global Core Graphics coordinates (points, origin at
/// the top-left of the main display, y grows downward).
struct DisplayInfo: Equatable {
    var id: CGDirectDisplayID
    var frame: CGRect
    var name: String

    /// Active displays, skipping mirrors so each surface appears once.
    static func current() -> [DisplayInfo] {
        var count: UInt32 = 0
        guard CGGetActiveDisplayList(0, nil, &count) == .success, count > 0 else { return [] }
        var ids = [CGDirectDisplayID](repeating: 0, count: Int(count))
        guard CGGetActiveDisplayList(count, &ids, &count) == .success else { return [] }

        let names: [CGDirectDisplayID: String] = Dictionary(
            NSScreen.screens.compactMap { screen in
                guard let number = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber else {
                    return nil
                }
                return (CGDirectDisplayID(number.uint32Value), screen.localizedName)
            },
            uniquingKeysWith: { first, _ in first }
        )

        return ids.prefix(Int(count)).compactMap { id in
            if CGDisplayMirrorsDisplay(id) != kCGNullDirectDisplay { return nil }
            let name = names[id] ?? (CGDisplayIsBuiltin(id) != 0 ? "Built-in Display" : "Display")
            return DisplayInfo(id: id, frame: CGDisplayBounds(id), name: name)
        }
    }
}
