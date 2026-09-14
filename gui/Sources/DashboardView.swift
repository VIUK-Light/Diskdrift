import AppKit
import SwiftUI

struct DashboardView: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("DiskDrift").font(.largeTitle.bold())
                    Text("Your disk is full. DiskDrift tells you why.")
                        .foregroundStyle(.secondary)
                }

                volumeCard
                growthCard
                actionsCard
            }
            .padding(24)
            .frame(maxWidth: 820, alignment: .leading)
        }
        .navigationTitle("Dashboard")
    }

    private var volumeCard: some View {
        Card(title: "This Mac") {
            if let usage = model.diskUsage {
                let used = usage.usedBytes
                let total = max(usage.totalBytes, 1)
                VStack(alignment: .leading, spacing: 10) {
                    HStack(alignment: .firstTextBaseline) {
                        Text(formatBytes(used)).font(.system(size: 34, weight: .semibold, design: .rounded))
                        Text("used of \(formatBytes(usage.totalBytes))")
                            .foregroundStyle(.secondary)
                    }
                    ProgressView(value: Double(used), total: Double(total))
                        .progressViewStyle(.linear)
                    Text("Free \(formatBytes(usage.availableBytes))")
                        .foregroundStyle(.secondary)
                    Text(displayPath(usage.path))
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
            } else {
                HStack(spacing: 10) {
                    ProgressView().controlSize(.small)
                    Text("Reading volume…").foregroundStyle(.secondary)
                }
            }
        }
    }

    private var growthCard: some View {
        Card(title: "Last 24 hours") {
            if let what = model.whatHappened {
                if what.total.eventCount == 0 {
                    Text("No watch events recorded yet.")
                        .foregroundStyle(.secondary)
                    Text("Run `diskdrift watch` in the terminal to record changes.")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                } else {
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text(formatDelta(what.total.deltaBytes))
                            .font(.system(size: 30, weight: .semibold, design: .rounded))
                            .foregroundColor(what.total.deltaBytes >= 0 ? .orange : .green)
                        Text("across \(what.total.eventCount) events")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(what.incidents.prefix(5)) { incident in
                        HStack {
                            Text(incident.label).lineLimit(1)
                            Spacer()
                            DeltaLabel(bytes: incident.deltaBytes, font: .callout)
                        }
                    }
                }
            } else if let message = model.growthMessage {
                Text(message).foregroundStyle(.secondary)
                Text("Run `diskdrift watch` in the terminal to record changes.")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            } else {
                HStack(spacing: 10) {
                    ProgressView().controlSize(.small)
                    Text("Loading events…").foregroundStyle(.secondary)
                }
            }
        }
    }

    private var actionsCard: some View {
        Card(title: "Actions") {
            HStack(spacing: 12) {
                Button {
                    model.runScan()
                } label: {
                    Label("Scan now", systemImage: "arrow.clockwise")
                }
                Button {
                    model.takeSnapshot()
                } label: {
                    Label("Take snapshot", systemImage: "camera")
                }
                Button {
                    NSWorkspace.shared.open(URL(fileURLWithPath: model.dataDir))
                } label: {
                    Label("Open data folder", systemImage: "folder")
                }
                Spacer()
            }
            .disabled(model.isBusy)
        }
    }
}
