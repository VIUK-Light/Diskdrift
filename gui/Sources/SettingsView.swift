import SwiftUI

struct SettingsView: View {
    @AppStorage(SettingsKeys.menuBarMode) private var mode = MenuBarMode.free.rawValue
    @AppStorage(SettingsKeys.refreshMinutes) private var refreshMinutes = 5
    @AppStorage(SettingsKeys.notificationsEnabled) private var notifications = true
    @AppStorage(SettingsKeys.alertFreeBelowGB) private var alertFree = 20.0
    @AppStorage(SettingsKeys.alertGrowthPerHourGB) private var alertHour = 0.0
    @AppStorage(SettingsKeys.alertGrowthPerDayGB) private var alertDay = 0.0

    var body: some View {
        Form {
            Section("Menu bar") {
                Picker("Show", selection: $mode) {
                    ForEach(MenuBarMode.allCases) { option in
                        Text(option.label).tag(option.rawValue)
                    }
                }
                Picker("Refresh every", selection: $refreshMinutes) {
                    Text("1 minute").tag(1.0)
                    Text("5 minutes").tag(5.0)
                    Text("15 minutes").tag(15.0)
                    Text("30 minutes").tag(30.0)
                }
            }

            Section("Alerts") {
                Toggle("Send local notifications", isOn: $notifications)
                HStack {
                    Text("Warn when free space is below")
                    Spacer()
                    TextField("GB", value: $alertFree, format: .number)
                        .frame(width: 70)
                        .multilineTextAlignment(.trailing)
                    Text("GB")
                }
                HStack {
                    Text("Warn when growth exceeds")
                    Spacer()
                    TextField("GB/h", value: $alertHour, format: .number)
                        .frame(width: 70)
                        .multilineTextAlignment(.trailing)
                    Text("GB/h")
                }
                HStack {
                    Text("Warn when daily growth exceeds")
                    Spacer()
                    TextField("GB/day", value: $alertDay, format: .number)
                        .frame(width: 70)
                        .multilineTextAlignment(.trailing)
                    Text("GB/day")
                }
                Text("Set a value to 0 to disable that alert. Alerts are evaluated locally; nothing leaves this Mac.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(20)
        .frame(width: 460)
    }
}
