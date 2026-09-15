// UserDefaults-backed settings shared by the menu bar and settings screens.

import Foundation

enum MenuBarMode: String, CaseIterable, Identifiable {
    case free
    case change
    case usage

    var id: String { rawValue }

    var label: String {
        switch self {
        case .free: return "Free space"
        case .change: return "Today's change"
        case .usage: return "Disk usage"
        }
    }
}

enum SettingsKeys {
    static let menuBarMode = "menuBarMode"
    static let refreshMinutes = "refreshMinutes"
    static let notificationsEnabled = "notificationsEnabled"
    static let alertFreeBelowGB = "alertFreeBelowGB"
    static let alertGrowthPerHourGB = "alertGrowthPerHourGB"
    static let alertGrowthPerDayGB = "alertGrowthPerDayGB"

    static func registerDefaults() {
        UserDefaults.standard.register(defaults: [
            menuBarMode: MenuBarMode.free.rawValue,
            refreshMinutes: 5,
            notificationsEnabled: true,
            alertFreeBelowGB: 20.0,
            alertGrowthPerHourGB: 0.0,
            alertGrowthPerDayGB: 0.0,
        ])
    }
}
