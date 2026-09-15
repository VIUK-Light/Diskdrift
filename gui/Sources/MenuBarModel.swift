// Menu bar state: periodic local refreshes, display modes and threshold
// alerts. Everything stays on this Mac; nothing is sent anywhere.

import AppKit
import Foundation
import UserNotifications

struct MenuAlert: Identifiable {
    let id: String
    let title: String
    let message: String
    let date: Date
}

@MainActor
final class MenuBarModel: ObservableObject {
    @Published var usage: DiskUsage?
    @Published var todayChange: Int64 = 0
    @Published var incidents: [Incident] = []
    @Published var lastRefresh: Date?
    @Published var alert: MenuAlert?

    private var timer: Timer?
    private var observer: NSObjectProtocol?
    private var conditionActive: [String: Bool] = [:]
    private var lastAlertAt: [String: Date] = [:]
    private var permissionRequested = false
    private var notificationAuthorized = false

    func start() {
        if observer == nil {
            observer = NotificationCenter.default.addObserver(
                forName: UserDefaults.didChangeNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                Task { @MainActor in self?.settingsChanged() }
            }
        }
        refresh()
        scheduleTimer()
    }

    func stop() {
        timer?.invalidate()
        timer = nil
        if let observer {
            NotificationCenter.default.removeObserver(observer)
            self.observer = nil
        }
    }

    func refresh() {
        let home = NSHomeDirectory()
        do {
            usage = try Core.diskUsage(path: home)
        } catch {
            // keep the previous value
        }

        var today: WhatHappenedResult?
        do {
            today = try Core.whatHappened(
                home: home, dataDir: nil, since: nil, from: "00:00", to: nil, limit: 5)
            todayChange = today?.total.deltaBytes ?? 0
            incidents = today?.incidents ?? []
        } catch {
            todayChange = 0
            incidents = []
        }
        lastRefresh = Date()
        evaluateAlerts(today: today)
    }

    var title: String {
        let mode =
            UserDefaults.standard.string(forKey: SettingsKeys.menuBarMode)
            ?? MenuBarMode.free.rawValue
        switch MenuBarMode(rawValue: mode) ?? .free {
        case .change:
            return formatDelta(todayChange)
        case .usage:
            guard let usage else { return "--" }
            return "\(formatBytes(usage.usedBytes)) / \(formatBytes(usage.totalBytes))"
        case .free:
            guard let usage else { return "--" }
            return formatBytes(usage.availableBytes)
        }
    }

    // MARK: - Alerts

    private func evaluateAlerts(today: WhatHappenedResult?) {
        let defaults = UserDefaults.standard

        if let usage {
            let threshold = defaults.double(forKey: SettingsKeys.alertFreeBelowGB)
            let freeGB = Double(usage.availableBytes) / 1_000_000_000
            checkCondition(
                key: "free",
                active: threshold > 0 && freeGB < threshold,
                title: "Free space is low",
                message:
                    "Only \(formatBytes(usage.availableBytes)) free "
                    + "(threshold \(String(format: "%.0f", threshold)) GB)."
            )
        }

        if let today {
            let threshold = defaults.double(forKey: SettingsKeys.alertGrowthPerDayGB)
            let growthGB = Double(max(today.total.deltaBytes, 0)) / 1_000_000_000
            checkCondition(
                key: "day",
                active: threshold > 0 && growthGB > threshold,
                title: "Storage grew today",
                message:
                    "Today's usage increased by \(formatDelta(today.total.deltaBytes))."
                    + cause(from: today.incidents)
            )
        }

        let hourThreshold = defaults.double(forKey: SettingsKeys.alertGrowthPerHourGB)
        if hourThreshold > 0 {
            do {
                let hour = try Core.whatHappened(
                    home: NSHomeDirectory(), dataDir: nil, since: "1h", from: nil, to: nil,
                    limit: 3)
                let growthGB = Double(max(hour.total.deltaBytes, 0)) / 1_000_000_000
                checkCondition(
                    key: "hour",
                    active: growthGB > hourThreshold,
                    title: "Storage is growing quickly",
                    message:
                        "The last hour added \(formatDelta(hour.total.deltaBytes))."
                        + cause(from: hour.incidents)
                )
            } catch {
                // no event database yet
            }
        }
    }

    private func cause(from incidents: [Incident]) -> String {
        guard let top = incidents.first, top.deltaBytes > 0 else { return "" }
        return " Main cause: \(top.label) \(formatDelta(top.deltaBytes))."
    }

    private func checkCondition(key: String, active: Bool, title: String, message: String) {
        let wasActive = conditionActive[key] ?? false
        conditionActive[key] = active
        guard active else { return }
        // Cooldown: one alert per condition per hour while it stays active.
        if wasActive, let last = lastAlertAt[key], Date().timeIntervalSince(last) < 3_600 {
            return
        }
        lastAlertAt[key] = Date()
        let alert = MenuAlert(id: key, title: title, message: message, date: Date())
        self.alert = alert
        postNotification(alert)
    }

    private func postNotification(_ alert: MenuAlert) {
        guard UserDefaults.standard.bool(forKey: SettingsKeys.notificationsEnabled) else {
            return
        }
        if !permissionRequested {
            permissionRequested = true
            UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) {
                granted, _ in
                Task { @MainActor in
                    self.notificationAuthorized = granted
                    if granted { self.deliver(alert) }
                }
            }
            return
        }
        if notificationAuthorized {
            deliver(alert)
        }
    }

    private func deliver(_ alert: MenuAlert) {
        let content = UNMutableNotificationContent()
        content.title = alert.title
        content.body = alert.message
        let request = UNNotificationRequest(
            identifier: UUID().uuidString, content: content, trigger: nil)
        UNUserNotificationCenter.current().add(request)
    }

    func dismissAlert() {
        alert = nil
    }

    // MARK: - Settings changes

    private func settingsChanged() {
        scheduleTimer()
        objectWillChange.send()
    }

    private func scheduleTimer() {
        timer?.invalidate()
        let minutes = max(1.0, UserDefaults.standard.double(forKey: SettingsKeys.refreshMinutes))
        timer = Timer.scheduledTimer(withTimeInterval: minutes * 60, repeats: true) {
            [weak self] _ in
            Task { @MainActor in self?.refresh() }
        }
    }
}

func relativeTime(_ date: Date) -> String {
    let seconds = Int(Date().timeIntervalSince(date))
    if seconds < 60 { return "just now" }
    if seconds < 3_600 { return "\(seconds / 60) min ago" }
    if seconds < 86_400 { return "\(seconds / 3_600) h ago" }
    return "\(seconds / 86_400) d ago"
}
