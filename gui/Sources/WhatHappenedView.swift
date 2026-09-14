import SwiftUI

struct WhatHappenedView: View {
    @EnvironmentObject var model: AppModel
    @State private var window = "24h"

    private let options = ["1h", "24h", "7d"]

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Picker("Window", selection: $window) {
                ForEach(options, id: \.self) { option in
                    Text("Last \(option)").tag(option)
                }
            }
            .pickerStyle(.segmented)
            .frame(maxWidth: 360)

            if let what = model.whatHappened {
                if what.total.eventCount == 0 {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("No changes recorded in \(what.window).")
                            .font(.title3.bold())
                        Text("Run `diskdrift watch` in the terminal to record changes.")
                            .foregroundStyle(.secondary)
                    }
                } else {
                    VStack(alignment: .leading, spacing: 6) {
                        HStack(alignment: .firstTextBaseline, spacing: 10) {
                            Text(
                                what.total.deltaBytes >= 0
                                    ? "Disk usage increased by"
                                    : "Disk usage decreased by"
                            )
                            .font(.title3)
                            Text(formatBytes(UInt64(abs(what.total.deltaBytes))))
                                .font(.system(size: 30, weight: .semibold, design: .rounded))
                                .foregroundColor(
                                    what.total.deltaBytes >= 0 ? .orange : .green)
                        }
                        Text("\(what.total.eventCount) events · \(what.window)")
                            .foregroundStyle(.secondary)
                    }

                    List(Array(what.incidents.enumerated()), id: \.offset) { index, incident in
                        HStack(alignment: .top, spacing: 14) {
                            Text("\(index + 1).")
                                .font(.headline.monospacedDigit())
                                .foregroundStyle(.secondary)
                            VStack(alignment: .leading, spacing: 3) {
                                Text(incident.label).font(.headline)
                                Text(timeSpan(incident))
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                Text("\(incident.eventCount) events · \(incident.categoryId)")
                                    .font(.caption2)
                                    .foregroundStyle(.tertiary)
                            }
                            Spacer()
                            DeltaLabel(bytes: incident.deltaBytes, font: .title3)
                        }
                        .padding(.vertical, 4)
                    }
                    .listStyle(.inset)
                }
            } else {
                HStack(spacing: 10) {
                    ProgressView().controlSize(.small)
                    Text("Loading events…").foregroundStyle(.secondary)
                }
            }
        }
        .padding(24)
        .navigationTitle("What Happened")
        .onChange(of: window) { value in
            model.loadWhatHappened(since: value)
        }
    }

    private func timeSpan(_ incident: Incident) -> String {
        let first = shortTime(incident.firstEvent)
        let last = shortTime(incident.lastEvent)
        if first == last { return first }
        return "\(first)–\(last)"
    }
}
