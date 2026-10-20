import Foundation

/// Command-line switches, mostly for development:
///
/// * `--profile <name>` keeps a separate identity, configuration and settings,
///   so two copies can run side by side on one Mac.
/// * `--role primary|secondary` skips the role question.
/// * `--dry-run` logs received input instead of posting it.
struct LaunchOptions {
    var profile: String?
    var role: Role?
    var dryRun = false

    static func parse(_ args: [String]) -> LaunchOptions {
        var options = LaunchOptions()
        var it = args.dropFirst().makeIterator()
        while let arg = it.next() {
            switch arg {
            case "--profile":
                options.profile = it.next().flatMap { $0.isEmpty ? nil : $0 }
            case "--role":
                options.role = it.next().flatMap(Role.init(rawValue:))
            case "--dry-run":
                options.dryRun = true
            default:
                break
            }
        }
        return options
    }

    /// `~/Library/Application Support/Edgeway[/Profiles/<name>]`
    var dataDirectory: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Edgeway", isDirectory: true)
        guard let profile else { return base }
        return base.appendingPathComponent("Profiles", isDirectory: true).appendingPathComponent(profile, isDirectory: true)
    }

    var defaults: UserDefaults {
        guard let profile else { return .standard }
        return UserDefaults(suiteName: "io.github.rafeuddaraj.Edgeway.\(profile)") ?? .standard
    }
}

enum Role: String, CaseIterable, Identifiable {
    /// Shares this Mac's keyboard and trackpad with other computers.
    case primary
    /// Is controlled by another computer.
    case secondary

    var id: String { rawValue }
}
