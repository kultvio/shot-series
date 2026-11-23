# ShotSeries for Windows

English · [Русский](README.ru.md)

This preview targets Windows 11 x64. ShotSeries adds an icon to the notification area and saves the selected region as a PNG when you press **F8**.

1. Open `ShotSeries.exe`. If the panel is hidden, click the ShotSeries icon next to the clock.
2. Click **Select area** (**Выделить область**) and drag over the part of the screen you want to capture. Drag an edge or corner to resize the frame. Press Esc or right-click to cancel selection.
3. Choose a parent folder and press **+**. In an empty folder, ShotSeries creates and selects `Серия 1` (Series 1). Press **+** again to start `Серия 2` (Series 2).
4. Press **F8** after each step. The screenshots appear as `1.png`, `2.png`, and so on. Change the hotkey next to **Capture** (**Снять кадр**).

Clicks and scrolling pass through the shade to the app underneath. The resize handles remain interactive. Each region belongs to one display. When taking a screenshot, ShotSeries briefly hides its own windows. The region and active series are saved between launches.

To quit, right-click the notification area icon and choose **Quit** (**Выйти**).

## Build from source on Windows

Install Rust 1.85 or newer and Visual Studio Build Tools with the **Desktop development with C++** workload. Run the following command from the `Windows` folder.

```powershell
cargo build --release --locked
```

Cargo writes the executable to `target/release/shotseries.exe`.
