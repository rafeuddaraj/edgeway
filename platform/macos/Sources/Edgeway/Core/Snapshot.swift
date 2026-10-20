import Foundation

/// The engine's state, decoded from `ew_engine_snapshot`.
struct Snapshot: Decodable, Equatable {
    var role: String
    var name: String
    var id: String
    var shortId: String
    var paused: Bool
    /// Primary: the device receiving input, or nil for this Mac.
    var focus: String?
    /// Secondary: the device controlling this Mac right now.
    var controlledBy: String?
    var edgeResistance: Double
    var port: Int
    var screens: [ScreenRecord]
    var devices: [DeviceRecord]
    var nearby: [NearbyRecord]
    var pairing: PairingRecord?

    static let empty = Snapshot(
        role: "", name: "", id: "", shortId: "", paused: false, focus: nil, controlledBy: nil,
        edgeResistance: 40, port: 0, screens: [], devices: [], nearby: [], pairing: nil
    )

    var connectedDevices: [DeviceRecord] { devices.filter(\.isConnected) }

    func device(id: String?) -> DeviceRecord? {
        guard let id else { return nil }
        return devices.first { $0.id == id }
    }
}

struct ScreenRecord: Decodable, Equatable, Hashable {
    var x: Double
    var y: Double
    var w: Double
    var h: Double
    var name: String

    var rect: CGRect { CGRect(x: x, y: y, width: w, height: h) }
}

struct Placement: Decodable, Equatable, Hashable {
    var x: Double
    var y: Double
}

struct DeviceRecord: Decodable, Equatable, Identifiable, Hashable {
    var id: String
    var shortId: String
    var name: String
    var platform: String
    var status: String
    var online: Bool
    var rttMs: Double?
    var error: String?
    var placement: Placement?
    var screens: [ScreenRecord]

    var isConnected: Bool { status == "connected" }
    var isConnecting: Bool { status == "connecting" }

    /// SF Symbol for the kind of computer.
    var symbol: String {
        switch platform {
        case "windows", "linux": return "pc"
        default:
            let lower = name.lowercased()
            if lower.contains("macbook") { return "laptopcomputer" }
            if lower.contains("mini") { return "macmini" }
            if lower.contains("studio") { return "macstudio" }
            if lower.contains("imac") { return "desktopcomputer" }
            if lower.contains("pro") { return "macpro.gen3" }
            return "desktopcomputer"
        }
    }

    var statusText: String {
        if isConnected {
            if let rtt = rttMs { return "Connected · \(max(1, Int(rtt.rounded()))) ms" }
            return "Connected"
        }
        if isConnecting { return "Connecting…" }
        return online ? "Available" : "Not on this network"
    }
}

struct NearbyRecord: Decodable, Equatable, Identifiable, Hashable {
    var instance: String
    var fingerprint: String
    var platform: String

    var id: String { instance }
}

struct PairingRecord: Decodable, Equatable {
    /// `show_code`, `enter_code` or `working`.
    var state: String
    var peer: String
    var code: String?
}
