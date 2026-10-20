import SwiftUI

/// One device's displays in arrangement space (this Mac's coordinates).
struct ArrangementGroup: Identifiable {
    var id: String
    var name: String
    var subtitle: String
    var isLocal: Bool
    var dimmed: Bool
    var device: DeviceRecord?
    /// Each display, in arrangement coordinates.
    var screens: [(rect: CGRect, name: String, isMain: Bool)]
    var bounds: CGRect
}

/// Builds the groups shown in the arrangement view and snaps dragged devices
/// against this Mac's displays, like System Settings › Displays.
struct ArrangementLayout {
    var groups: [ArrangementGroup] = []

    init(snapshot: Snapshot, pending: [String: CGPoint]) {
        let local = snapshot.screens.map { ($0.rect, $0.name, $0.x == 0 && $0.y == 0) }
        if let b = Self.union(local.map(\.0)) {
            groups.append(ArrangementGroup(
                id: "local", name: snapshot.name, subtitle: "This Mac", isLocal: true, dimmed: false,
                device: nil, screens: local, bounds: b
            ))
        }
        for device in snapshot.devices where !device.screens.isEmpty {
            guard let origin = pending[device.id] ?? device.placement.map({ CGPoint(x: $0.x, y: $0.y) }),
                  let own = Self.union(device.screens.map(\.rect)) else { continue }
            let dx = origin.x - own.minX
            let dy = origin.y - own.minY
            let screens = device.screens.map { ($0.rect.offsetBy(dx: dx, dy: dy), $0.name, $0.x == 0 && $0.y == 0) }
            groups.append(ArrangementGroup(
                id: device.id, name: device.name, subtitle: device.statusText, isLocal: false,
                dimmed: !device.isConnected, device: device, screens: screens,
                bounds: own.offsetBy(dx: dx, dy: dy)
            ))
        }
    }

    var localScreens: [CGRect] { groups.first(where: \.isLocal)?.screens.map(\.rect) ?? [] }

    static func union(_ rects: [CGRect]) -> CGRect? {
        guard var u = rects.first else { return nil }
        for r in rects.dropFirst() { u = u.union(r) }
        return u
    }

    struct Fit {
        var scale: CGFloat
        var offset: CGPoint

        func point(_ p: CGPoint) -> CGPoint {
            CGPoint(x: p.x * scale + offset.x, y: p.y * scale + offset.y)
        }
    }

    /// Scale and offset that fit everything in `size` with some breathing room.
    func fit(in size: CGSize, padding: CGFloat) -> Fit {
        guard let all = Self.union(groups.map(\.bounds)), all.width > 0, all.height > 0 else {
            return Fit(scale: 0.1, offset: .zero)
        }
        // Leave room around the content so a device can be dragged to any side.
        let canvas = all.insetBy(dx: -all.width * 0.15, dy: -all.height * 0.15)
        let scale = min((size.width - padding * 2) / canvas.width, (size.height - padding * 2) / canvas.height)
        let offset = CGPoint(
            x: (size.width - canvas.width * scale) / 2 - canvas.minX * scale,
            y: (size.height - canvas.height * scale) / 2 - canvas.minY * scale
        )
        return Fit(scale: scale, offset: offset)
    }

    /// Moves a device of `size` proposed at `origin` so it touches the nearest
    /// edge of one of this Mac's displays with a usable overlap, then aligns
    /// it to centres and edges that are close.
    static func snap(origin: CGPoint, size: CGSize, local: [CGRect], threshold: CGFloat) -> CGPoint {
        var best: (CGPoint, CGFloat)?
        for s in local {
            let minOverlapX = min(size.width, s.width) * 0.25
            let minOverlapY = min(size.height, s.height) * 0.25
            let clampX = { (x: CGFloat) in min(max(x, s.minX - size.width + minOverlapX), s.maxX - minOverlapX) }
            let clampY = { (y: CGFloat) in min(max(y, s.minY - size.height + minOverlapY), s.maxY - minOverlapY) }
            let candidates = [
                CGPoint(x: clampX(origin.x), y: s.minY - size.height), // above
                CGPoint(x: clampX(origin.x), y: s.maxY), // below
                CGPoint(x: s.minX - size.width, y: clampY(origin.y)), // left
                CGPoint(x: s.maxX, y: clampY(origin.y)), // right
            ]
            for c in candidates {
                let rect = CGRect(origin: c, size: size)
                if local.contains(where: { $0.intersects(rect.insetBy(dx: 0.5, dy: 0.5)) }) { continue }
                let d = hypot(c.x - origin.x, c.y - origin.y)
                if best == nil || d < best!.1 { best = (aligned(c, size: size, to: s, threshold: threshold), d) }
            }
        }
        return best.map { CGPoint(x: $0.0.x.rounded(), y: $0.0.y.rounded()) } ?? origin
    }

    private static func aligned(_ p: CGPoint, size: CGSize, to s: CGRect, threshold: CGFloat) -> CGPoint {
        var p = p
        let horizontal = p.y + size.height <= s.minY + 0.5 || p.y >= s.maxY - 0.5
        if horizontal {
            for x in [s.midX - size.width / 2, s.minX, s.maxX - size.width] where abs(p.x - x) < threshold {
                p.x = x
                break
            }
        } else {
            for y in [s.midY - size.height / 2, s.minY, s.maxY - size.height] where abs(p.y - y) < threshold {
                p.y = y
                break
            }
        }
        return p
    }
}

/// Drag-and-drop arrangement of all devices' displays.
struct ArrangementView: View {
    @EnvironmentObject private var model: AppModel
    @State private var dragging: String?
    @State private var dragOffset: CGSize = .zero
    @State private var pending: [String: CGPoint] = [:]
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        GeometryReader { geo in
            let layout = ArrangementLayout(snapshot: model.snapshot, pending: pending)
            let fit = layout.fit(in: geo.size, padding: 24)
            ZStack(alignment: .topLeading) {
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(Color(nsColor: .controlBackgroundColor))
                    .overlay(
                        RoundedRectangle(cornerRadius: 10, style: .continuous)
                            .strokeBorder(Color(nsColor: .separatorColor))
                    )

                if layout.groups.count < 2 {
                    Text(model.snapshot.devices.isEmpty
                         ? "Pair a device and it will appear here."
                         : "Your devices appear here after they connect for the first time.")
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .center)
                        .padding(.top, 14)
                }

                ForEach(layout.groups) { group in
                    let isDragging = dragging == group.id
                    let size = CGSize(width: group.bounds.width * fit.scale, height: group.bounds.height * fit.scale)
                    let center = fit.point(CGPoint(x: group.bounds.midX, y: group.bounds.midY))
                    DeviceGroupView(group: group, scale: fit.scale, highlighted: isDragging)
                        .frame(width: size.width, height: size.height)
                        .position(x: center.x + (isDragging ? dragOffset.width : 0), y: center.y + (isDragging ? dragOffset.height : 0))
                        .zIndex(isDragging ? 2 : group.isLocal ? 0 : 1)
                        .gesture(dragGesture(group, layout: layout, scale: fit.scale), including: group.isLocal ? .none : .all)
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel("\(group.name), \(group.subtitle)")
                        .accessibilityHint(group.isLocal ? "" : "Use the actions to move it next to this Mac.")
                        .accessibilityActions {
                            if !group.isLocal {
                                Button("Place Above This Mac") { placeBeside(group, layout: layout, dx: 0, dy: -1) }
                                Button("Place Below This Mac") { placeBeside(group, layout: layout, dx: 0, dy: 1) }
                                Button("Place Left of This Mac") { placeBeside(group, layout: layout, dx: -1, dy: 0) }
                                Button("Place Right of This Mac") { placeBeside(group, layout: layout, dx: 1, dy: 0) }
                            }
                        }
                }
            }
        }
        .onChange(of: model.snapshot) { snapshot in
            // Drop optimistic positions once the engine has caught up.
            pending = pending.filter { id, p in
                guard let placed = snapshot.device(id: id)?.placement else { return false }
                return abs(placed.x - p.x) > 0.5 || abs(placed.y - p.y) > 0.5
            }
        }
    }

    private func dragGesture(_ group: ArrangementGroup, layout: ArrangementLayout, scale: CGFloat) -> some Gesture {
        DragGesture(minimumDistance: 2)
            .onChanged { value in
                dragging = group.id
                dragOffset = value.translation
            }
            .onEnded { value in
                guard let device = group.device, scale > 0 else { return }
                let proposed = CGPoint(
                    x: group.bounds.minX + value.translation.width / scale,
                    y: group.bounds.minY + value.translation.height / scale
                )
                let snapped = ArrangementLayout.snap(
                    origin: proposed, size: group.bounds.size, local: layout.localScreens, threshold: 16 / scale
                )
                commit(device, id: group.id, origin: snapped)
            }
    }

    private func placeBeside(_ group: ArrangementGroup, layout: ArrangementLayout, dx: CGFloat, dy: CGFloat) {
        guard let device = group.device, let local = ArrangementLayout.union(layout.localScreens) else { return }
        let size = group.bounds.size
        let proposed = CGPoint(
            x: dx < 0 ? local.minX - size.width : dx > 0 ? local.maxX : local.midX - size.width / 2,
            y: dy < 0 ? local.minY - size.height : dy > 0 ? local.maxY : local.midY - size.height / 2
        )
        let snapped = ArrangementLayout.snap(origin: proposed, size: size, local: layout.localScreens, threshold: 1)
        commit(device, id: group.id, origin: snapped)
    }

    private func commit(_ device: DeviceRecord, id: String, origin: CGPoint) {
        withAnimation(reduceMotion ? nil : .spring(response: 0.28, dampingFraction: 0.86)) {
            pending[id] = origin
            dragging = nil
            dragOffset = .zero
        }
        model.place(device, at: origin)
    }
}

/// A device drawn as its displays, labelled with its name.
struct DeviceGroupView: View {
    var group: ArrangementGroup
    var scale: CGFloat
    var highlighted: Bool

    /// This Mac, in the brand's blues.
    private static let localFill = LinearGradient(
        colors: [Color(red: 0.11, green: 0.16, blue: 0.55), Color(red: 0.18, green: 0.42, blue: 1.0)],
        startPoint: .topLeading, endPoint: .bottomTrailing
    )
    /// Other devices, in a neutral graphite that keeps white labels legible.
    private static let remoteFill = LinearGradient(
        colors: [Color(red: 0.33, green: 0.36, blue: 0.42), Color(red: 0.50, green: 0.54, blue: 0.61)],
        startPoint: .topLeading, endPoint: .bottomTrailing
    )

    var body: some View {
        ZStack(alignment: .topLeading) {
            ForEach(Array(group.screens.enumerated()), id: \.offset) { _, screen in
                let frame = CGRect(
                    x: (screen.rect.minX - group.bounds.minX) * scale,
                    y: (screen.rect.minY - group.bounds.minY) * scale,
                    width: screen.rect.width * scale,
                    height: screen.rect.height * scale
                )
                ZStack(alignment: .top) {
                    RoundedRectangle(cornerRadius: 5, style: .continuous)
                        .fill(group.isLocal ? Self.localFill : Self.remoteFill)
                        .opacity(group.dimmed ? 0.55 : 1)
                    if screen.isMain {
                        // The menu bar, as System Settings draws it.
                        Rectangle()
                            .fill(.white.opacity(0.85))
                            .frame(height: max(2, min(4, frame.height * 0.04)))
                            .padding(.horizontal, 1)
                            .clipShape(RoundedRectangle(cornerRadius: 5, style: .continuous))
                    }
                    RoundedRectangle(cornerRadius: 5, style: .continuous)
                        .strokeBorder(highlighted ? Color.accentColor : Color.black.opacity(0.18), lineWidth: highlighted ? 2 : 1)
                }
                .frame(width: frame.width - 2, height: frame.height - 2)
                .offset(x: frame.minX + 1, y: frame.minY + 1)
                .help(screen.name)
            }
            VStack(spacing: 2) {
                Text(group.name)
                    .font(.system(size: 12, weight: .semibold))
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                Text(group.subtitle)
                    .font(.system(size: 10))
                    .opacity(0.85)
                    .lineLimit(1)
            }
            .foregroundStyle(.white)
            .shadow(color: .black.opacity(0.35), radius: 1, y: 0.5)
            .padding(6)
            .frame(width: group.bounds.width * scale, height: group.bounds.height * scale)
        }
        .shadow(color: .black.opacity(highlighted ? 0.25 : 0.12), radius: highlighted ? 8 : 3, y: highlighted ? 4 : 1)
        .contentShape(Rectangle())
    }
}
