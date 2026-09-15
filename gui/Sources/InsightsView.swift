import SwiftUI

struct InsightsView: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Storage Insights").font(.largeTitle.bold())
                    Text("What each area is, and how risky it is to review. DiskDrift never deletes anything.")
                        .foregroundStyle(.secondary)
                }

                if let insights = model.insights {
                    if insights.insights.isEmpty {
                        Text("Nothing to report yet. Run a scan first.")
                            .foregroundStyle(.secondary)
                    } else {
                        ForEach(insights.insights) { insight in
                            insightCard(insight)
                        }
                    }
                    if let large = model.largeFiles, !large.files.isEmpty {
                        Card(title: "Largest files") {
                            ForEach(large.files.prefix(10)) { file in
                                HStack {
                                    Text(displayPath(file.path))
                                        .lineLimit(1)
                                        .truncationMode(.middle)
                                    Spacer()
                                    Text(formatBytes(file.allocatedBytes))
                                        .font(.callout.monospacedDigit())
                                        .foregroundStyle(.secondary)
                                }
                            }
                        }
                    }
                } else if let message = model.insightsMessage {
                    Text(message).foregroundStyle(.secondary)
                } else {
                    HStack(spacing: 10) {
                        ProgressView().controlSize(.small)
                        Text("Scanning for insights…").foregroundStyle(.secondary)
                    }
                }
            }
            .padding(24)
            .frame(maxWidth: 860, alignment: .leading)
        }
        .navigationTitle("Insights")
        .onAppear {
            if model.insights == nil {
                model.loadInsights()
            }
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    model.loadInsights()
                } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                }
                .disabled(model.isBusy)
            }
        }
    }

    private func insightCard(_ insight: Insight) -> some View {
        Card(title: insight.title) {
            HStack(alignment: .firstTextBaseline) {
                Text(formatBytes(insight.allocatedBytes))
                    .font(.title3.bold().monospacedDigit())
                riskBadge(insight.risk)
                if insight.userData {
                    Text("may contain user data")
                        .font(.caption)
                        .foregroundStyle(.orange)
                }
                Spacer()
            }
            Text(insight.whatIsIt)
                .font(.callout)
                .foregroundStyle(.secondary)
            Text("Recommendation: \(insight.recommendation)")
                .font(.callout)
        }
    }

    private func riskBadge(_ risk: String) -> some View {
        Text(risk.uppercased())
            .font(.caption2.bold())
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(Capsule().fill(riskColor(risk).opacity(0.18)))
            .foregroundStyle(riskColor(risk))
    }

    private func riskColor(_ risk: String) -> Color {
        switch risk {
        case "low": return .green
        case "medium": return .orange
        case "high": return .red
        case "informational": return .secondary
        default: return .secondary
        }
    }
}
