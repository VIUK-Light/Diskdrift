// Observable application state. All work is delegated to the Rust core on a
// background task; the UI only renders the results.

import Foundation

@MainActor
final class AppModel: ObservableObject {
    @Published var diskUsage: DiskUsage?
    @Published var scan: ScanResult?
    @Published var history: HistoryResult?
    @Published var whatHappened: WhatHappenedResult?
    @Published var isBusy = false
    @Published var status = "Ready"
    @Published var errorMessage: String?
    @Published var historyMessage: String?
    @Published var growthMessage: String?

    let home = NSHomeDirectory()
    var dataDir: String {
        (home as NSString).appendingPathComponent("Library/Application Support/DiskDrift")
    }

    func refreshDashboard() {
        loadDiskUsage()
        loadWhatHappened(since: "24h")
    }

    func loadDiskUsage() {
        let home = self.home
        run({ try Core.diskUsage(path: home) }, done: { self.diskUsage = $0 })
    }

    func runScan(path: String? = nil, depth: Int = 0) {
        let home = self.home
        status = "Scanning storage…"
        run(
            { try Core.scan(home: home, dataDir: nil, path: path, depth: depth, threads: 0) },
            done: { result in
                self.scan = result
                self.status =
                    "Scanned \(formatBytes(result.totals.allocatedBytes)) · "
                    + "\(result.categories.count) categories"
            })
    }

    func takeSnapshot() {
        let home = self.home
        status = "Taking snapshot…"
        run(
            { try Core.snapshot(home: home, dataDir: nil, path: nil, depth: 0, threads: 0) },
            done: { result in
                self.status =
                    "Snapshot #\(result.snapshot.id) saved · "
                    + formatBytes(result.snapshot.totalAllocatedBytes)
                self.loadHistory(category: self.selectedHistoryCategory)
            })
    }

    /// History and events are optional on first run: show an empty state
    /// instead of an error alert.
    func loadHistory(category: String?) {
        let home = self.home
        isBusy = true
        Task.detached(priority: .userInitiated) {
            do {
                let value = try Core.history(home: home, dataDir: nil, category: category)
                await MainActor.run {
                    self.history = value
                    self.historyMessage = nil
                    self.isBusy = false
                }
            } catch {
                await MainActor.run {
                    self.history = nil
                    self.historyMessage = error.localizedDescription
                    self.isBusy = false
                }
            }
        }
    }

    func loadWhatHappened(since: String) {
        let home = self.home
        isBusy = true
        Task.detached(priority: .userInitiated) {
            do {
                let value = try Core.whatHappened(
                    home: home, dataDir: nil, since: since, from: nil, to: nil, limit: 8)
                await MainActor.run {
                    self.whatHappened = value
                    self.growthMessage = nil
                    self.isBusy = false
                }
            } catch {
                await MainActor.run {
                    self.whatHappened = nil
                    self.growthMessage = error.localizedDescription
                    self.isBusy = false
                }
            }
        }
    }

    func clearError() {
        errorMessage = nil
    }

    /// The History screen keeps this in sync so snapshots refresh the view.
    var selectedHistoryCategory: String?

    private func run<T>(_ work: @escaping () throws -> T, done: @escaping (T) -> Void) {
        isBusy = true
        Task.detached(priority: .userInitiated) {
            do {
                let value = try work()
                await MainActor.run {
                    self.isBusy = false
                    self.status = "Ready"
                    done(value)
                }
            } catch {
                await MainActor.run {
                    self.isBusy = false
                    self.status = "Ready"
                    self.errorMessage = error.localizedDescription
                }
            }
        }
    }
}
