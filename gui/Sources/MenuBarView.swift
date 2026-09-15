import AppKit
import SwiftUI

struct MenuBarView: View {
    @EnvironmentObject var model: MenuBarModel
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                Image(systemName: "internaldrive")
                Text("DiskDrift").font(.headline)
                Spacer()
                Button {
                    model.refresh()
                } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.borderless)
                .help("Refresh now")
            }

            if let alert = model.alert {
                VStack(alignment: .leading, spacing: 3) {
                    HStack {
                        Label(alert.title, systemImage: "exclamationmark.triangle.fill")
                            .font(.callout.bold())
                            .foregroundColor(.orange)
                        Spacer()
                        Button {
                            model.dismissAlert()
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .foregroundStyle(.tertiary)
                        }
                        .buttonStyle(.borderless)
                    }
                    Text(alert.message)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .padding(10)
                .background(
                    RoundedRectangle(cornerRadius: 8)
                        .fill(Color.orange.opacity(0.12))
                )
            }

            if let usage = model.usage {
                row("Free", formatBytes(usage.availableBytes), emphasis: true)
                row("Used", "\(formatBytes(usage.usedBytes)) of \(formatBytes(usage.totalBytes))")
            } else {
                row("Volume", "unavailable")
            }
            row("Today", formatDelta(model.todayChange))

            if !model.incidents.isEmpty {
                Divider()
                Text("Biggest growth")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                ForEach(model.incidents.prefix(3)) { incident in
                    HStack {
                        Text(incident.label)
                            .font(.callout)
                            .lineLimit(1)
                        Spacer()
                        DeltaLabel(bytes: incident.deltaBytes, font: .caption)
                    }
                }
            }

            Divider()
            HStack {
                if let last = model.lastRefresh {
                    Text("Updated \(relativeTime(last))")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
                Spacer()
            }
            HStack(spacing: 8) {
                Button("Open DiskDrift") {
                    openWindow(id: "main")
                    NSApp.activate(ignoringOtherApps: true)
                }
                Spacer()
                Button("Quit") {
                    NSApplication.shared.terminate(nil)
                }
            }
        }
        .padding(14)
        .frame(width: 330)
        .onAppear { model.start() }
    }

    private func row(_ label: String, _ value: String, emphasis: Bool = false) -> some View {
        HStack {
            Text(label).foregroundStyle(.secondary)
            Spacer()
            Text(value)
                .font(emphasis ? .title3.bold().monospacedDigit() : .callout.monospacedDigit())
        }
    }
}
