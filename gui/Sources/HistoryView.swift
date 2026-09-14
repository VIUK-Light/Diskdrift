import SwiftUI

struct HistoryView: View {
    @EnvironmentObject var model: AppModel
    @State private var category = "All"

    private let options = ["All", "developer", "ai", "applications", "system"]

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Picker("Category", selection: $category) {
                ForEach(options, id: \.self) { option in
                    Text(option.capitalized).tag(option)
                }
            }
            .pickerStyle(.segmented)
            .frame(maxWidth: 520)

            if let history = model.history {
                if history.days.isEmpty {
                    Text("No snapshots yet. Take a snapshot to start History.")
                        .foregroundStyle(.secondary)
                } else {
                    ChartView(days: history.days)
                        .frame(height: 190)
                    table(history.days)
                }
            } else if let message = model.historyMessage {
                Text(message).foregroundStyle(.secondary)
                Button("Take a snapshot") { model.takeSnapshot() }
                    .disabled(model.isBusy)
            } else {
                HStack(spacing: 10) {
                    ProgressView().controlSize(.small)
                    Text("Loading history…").foregroundStyle(.secondary)
                }
            }
        }
        .padding(24)
        .navigationTitle("History")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    model.takeSnapshot()
                } label: {
                    Label("Take snapshot", systemImage: "camera")
                }
                .disabled(model.isBusy)
            }
        }
        .onChange(of: category) { value in
            model.selectedHistoryCategory = value == "All" ? nil : value
            model.loadHistory(category: value == "All" ? nil : value)
        }
    }

    private func table(_ days: [HistoryDay]) -> some View {
        Table(days) {
            TableColumn("Date") { day in
                Text(shortDate(day.date + "T00:00:00"))
            }
            TableColumn("Tracked") { day in
                Text(formatBytes(day.allocatedBytes))
                    .monospacedDigit()
            }
            TableColumn("Change") { day in
                if let change = day.changeBytes {
                    DeltaLabel(bytes: change)
                } else {
                    Text("—").foregroundStyle(.secondary)
                }
            }
            TableColumn("Snapshots") { day in
                Text("\(day.snapshotCount)").foregroundStyle(.secondary)
            }
        }
        .frame(maxHeight: 320)
    }
}

private struct ChartView: View {
    let days: [HistoryDay]

    var body: some View {
        let values = days.compactMap(\.changeBytes)
        let maxAbs = max(values.map(abs).max() ?? 1, 1)
        HStack(alignment: .bottom, spacing: 8) {
            ForEach(days) { day in
                VStack(spacing: 6) {
                    Spacer(minLength: 0)
                    if let change = day.changeBytes, change != 0 {
                        let height = CGFloat(abs(change)) / CGFloat(maxAbs) * 120
                        RoundedRectangle(cornerRadius: 3)
                            .fill(change >= 0 ? Color.orange.opacity(0.8) : Color.green.opacity(0.8))
                            .frame(width: 26, height: max(3, height))
                            .help(formatDelta(change))
                    } else {
                        RoundedRectangle(cornerRadius: 3)
                            .fill(Color.secondary.opacity(0.25))
                            .frame(width: 26, height: 3)
                    }
                    Text(String(day.date.suffix(5)))
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity)
            }
        }
        .padding(.vertical, 8)
    }
}
