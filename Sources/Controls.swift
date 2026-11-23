import SwiftUI

struct Controls: View {
    @ObservedObject var app: AppController

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            header
            regionControls
            folderControls
            captureControls
        }
        .padding(16)
        .frame(width: 348)
        .tint(.indigo)
    }

    private var header: some View {
        HStack(spacing: 10) {
            Image(systemName: "viewfinder")
                .font(.system(size: 18, weight: .semibold))
                .foregroundStyle(.indigo)
                .frame(width: 35, height: 35)
                .background(.indigo.opacity(0.14), in: RoundedRectangle(cornerRadius: 10))
                .accessibilityHidden(true)
            Text("ShotSeries")
                .font(.system(size: 21, weight: .semibold))
                .accessibilityAddTraits(.isHeader)
            Spacer()
            Menu {
                Button("Показать папку в Finder", systemImage: "folder") { app.revealFolder() }
                    .disabled(app.root == nil)
                Divider()
                Button("Выйти из ShotSeries", systemImage: "power") { NSApp.terminate(nil) }
            } label: {
                Image(systemName: "ellipsis.circle").font(.system(size: 17))
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .accessibilityLabel("Меню приложения")
        }
    }

    private var regionControls: some View {
        VStack(alignment: .leading, spacing: 9) {
            HStack {
                Text("Область").font(.subheadline.weight(.semibold))
                Spacer()
                if let size = app.regionSize {
                    Text("\(Int(size.width)) × \(Int(size.height)) пт")
                        .monospacedDigit()
                } else {
                    Text("Не выделена")
                }
            }
            .font(.subheadline)
            .foregroundStyle(.secondary)

            HStack(spacing: 8) {
                Button(action: app.beginDrawing) {
                    Label(app.drawing ? "Отменить выделение" : app.regionSize == nil ? "Выделить область" : "Выделить заново",
                          systemImage: "rectangle.dashed")
                        .frame(maxWidth: .infinity)
                }
                if app.regionSize != nil && !app.drawing {
                    let label = app.overlayVisible ? "Скрыть рамку" : "Показать рамку"
                    Button(action: app.toggleOverlay) {
                        Image(systemName: app.overlayVisible ? "eye" : "eye.slash")
                    }
                    .help(label)
                    .accessibilityLabel(label)
                }
            }
            .controlSize(.large)
        }
        .padding(11)
        .background(.quaternary.opacity(0.48), in: RoundedRectangle(cornerRadius: 12))
    }

    private var folderControls: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Image(systemName: "folder").foregroundStyle(.secondary)
                    .frame(width: 16)
                    .accessibilityHidden(true)
                Text(app.root?.lastPathComponent ?? "Каталог для снимков")
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .help(app.root?.path ?? "Выберите основной каталог")
                Spacer(minLength: 0)
                Button(app.root == nil ? "Выбрать…" : "Сменить…", action: app.chooseRoot)
            }
            HStack(spacing: 8) {
                Image(systemName: "folder.badge.plus").foregroundStyle(.secondary)
                    .frame(width: 16)
                    .accessibilityHidden(true)
                Menu {
                    if app.series.isEmpty {
                        Text("Добавьте папку кнопкой «+»")
                    }
                    ForEach(app.series, id: \.path) { folder in
                        Button {
                            app.chooseSeries(folder)
                        } label: {
                            if app.activeSeries?.path == folder.path {
                                Label(folder.lastPathComponent, systemImage: "checkmark")
                            } else {
                                Text(folder.lastPathComponent)
                            }
                        }
                    }
                } label: {
                    Text(app.activeSeries?.lastPathComponent ?? "Выбрать серию")
                        .lineLimit(1)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .disabled(app.series.isEmpty)
                .accessibilityLabel("Папка для текущих снимков")
                .accessibilityValue(app.activeSeries?.lastPathComponent ?? "Не выбрана")
                Button(action: app.addSeries) {
                    Image(systemName: "plus").font(.body.weight(.medium))
                }
                .disabled(app.root == nil)
                .help("Создать новую серию")
                .accessibilityLabel("Создать новую серию")
            }
        }
    }

    private var captureControls: some View {
        VStack(alignment: .leading, spacing: 10) {
            Divider()
            HStack(spacing: 8) {
                Button(action: app.capture) {
                    Label("Снять кадр", systemImage: "camera")
                        .fontWeight(.medium)
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .disabled(app.drawing || app.regionSize == nil || app.activeSeries == nil)

                Menu {
                    ForEach(CaptureKey.choices) { key in
                        Button { app.chooseKey(key) } label: {
                            if app.key.number == key.number {
                                Label(key.label, systemImage: "checkmark")
                            } else {
                                Text(key.label)
                            }
                        }
                    }
                } label: {
                    Text(app.key.label).monospaced()
                }
                .fixedSize()
                .help("Изменить клавишу снимка")
                .accessibilityLabel("Клавиша снимка")
                .accessibilityValue(app.key.label)
            }
            .controlSize(.large)

            if let message = app.message {
                Label(message.text, systemImage: message.isError ? "exclamationmark.circle" : "info.circle")
                    .font(.callout)
                    .foregroundStyle(message.isError ? Color.orange : Color.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if app.needsScreenAccess {
                Button("Открыть Системные настройки", action: app.openScreenSettings)
            }
        }
    }
}
