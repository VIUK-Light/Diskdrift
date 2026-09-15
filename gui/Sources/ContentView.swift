import AppKit
import SwiftUI

enum AppSection: String, CaseIterable, Identifiable {
    case dashboard = "Dashboard"
    case storage = "Storage"
    case history = "History"
    case whatHappened = "What Happened"
    case system = "System"
    case duplicates = "Duplicates"
    case insights = "Insights"

    var id: String { rawValue }

    var icon: String {
        switch self {
        case .dashboard: return "gauge.medium"
        case .storage: return "internaldrive"
        case .history: return "chart.bar"
        case .whatHappened: return "clock.arrow.circlepath"
        case .system: return "externaldrive"
        case .duplicates: return "doc.on.doc"
        case .insights: return "lightbulb"
        }
    }
}

struct ContentView: View {
    @EnvironmentObject var model: AppModel
    @State private var selection: AppSection? = .dashboard

    var body: some View {
        NavigationSplitView {
            List(AppSection.allCases, selection: $selection) { section in
                Label(section.rawValue, systemImage: section.icon)
                    .tag(section)
            }
            .listStyle(.sidebar)
            .navigationSplitViewColumnWidth(min: 180, ideal: 200)
            .safeAreaInset(edge: .bottom) { statusBar }
        } detail: {
            detail
        }
        .alert(
            "DiskDrift",
            isPresented: Binding(
                get: { model.errorMessage != nil },
                set: { if !$0 { model.clearError() } }
            )
        ) {
            Button("OK") { model.clearError() }
        } message: {
            Text(model.errorMessage ?? "")
        }
    }

    @ViewBuilder
    private var detail: some View {
        switch selection ?? .dashboard {
        case .dashboard: DashboardView()
        case .storage: StorageView()
        case .history: HistoryView()
        case .whatHappened: WhatHappenedView()
        case .system: SystemView()
        case .duplicates: DuplicatesView()
        case .insights: InsightsView()
        }
    }

    private var statusBar: some View {
        HStack(spacing: 8) {
            if model.isBusy {
                ProgressView().controlSize(.small)
            }
            Text(model.status)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
            Spacer()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
    }
}
