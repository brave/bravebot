// A menu bar item for maintainers that lists the pull requests `make review-ready` reports.
//
// The list is cached in ~/Library/Caches/bravebot-review-ready/cache.json and read at launch, so the
// menu has content as soon as the app starts and opens at once when clicked. `make review-ready`
// runs in the background on a timer, after the Mac wakes, and on "Refresh Now". The number of ready
// pull requests is shown next to the icon. The menu lists them in the order `make review-ready`
// prints them, the one that has waited longest first, and does not sort them again.
//
// Settings come from Info.plist (written by install.sh) and can be overridden by environment
// variables for running the binary by hand:
//   ReviewReadyRepoPath         / REVIEW_READY_REPO      the bravebot checkout to run make in
//   ReviewReadyIntervalSeconds  / REVIEW_READY_INTERVAL  seconds between refreshes (default 300)
//   ReviewReadyMakeArguments    / REVIEW_READY_MAKE_ARGS extra make arguments, e.g. "REVIEWER=login"
//   ReviewReadyPath             / REVIEW_READY_PATH      directories to search for gh and python3 first

import AppKit

struct Item: Codable, Equatable {
    let url: String
    let title: String

    /// The pull request number, from the last component of its URL.
    var number: String { url.split(separator: "/").last.map(String.init) ?? "" }
}

struct Cache: Codable {
    var updatedAt: Date
    var items: [Item]
}

enum Settings {
    static func value(_ plistKey: String, _ environmentKey: String) -> String? {
        if let value = ProcessInfo.processInfo.environment[environmentKey], !value.isEmpty { return value }
        return Bundle.main.object(forInfoDictionaryKey: plistKey) as? String
    }

    static var repoPath: String {
        value("ReviewReadyRepoPath", "REVIEW_READY_REPO") ?? FileManager.default.currentDirectoryPath
    }

    static var interval: TimeInterval {
        let seconds = value("ReviewReadyIntervalSeconds", "REVIEW_READY_INTERVAL").flatMap(Double.init) ?? 300
        return max(seconds, 30)
    }

    static var makeArguments: [String] {
        (value("ReviewReadyMakeArguments", "REVIEW_READY_MAKE_ARGS") ?? "")
            .split(separator: " ").map(String.init)
    }

    /// The directories install.sh found gh and python3 in, then the usual places for them.
    static var searchPath: String {
        let usual = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
        guard let found = value("ReviewReadyPath", "REVIEW_READY_PATH"), !found.isEmpty else { return usual }
        return found + ":" + usual
    }

    static var cacheURL: URL {
        let base = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
        return base.appendingPathComponent("bravebot-review-ready/cache.json")
    }
}

struct RunResult {
    var status: Int32
    var out: String
    var err: String
}

/// Runs `make review-ready` in the checkout. A menu bar app does not inherit the shell's PATH, so
/// the places gh and python3 are looked for are named in Settings.searchPath.
func runReviewReady() -> RunResult {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/make")
    process.arguments = ["--no-print-directory", "-C", Settings.repoPath, "review-ready"] + Settings.makeArguments
    var environment = ProcessInfo.processInfo.environment
    environment["PATH"] = Settings.searchPath
    process.environment = environment
    let outPipe = Pipe()
    let errPipe = Pipe()
    process.standardOutput = outPipe
    process.standardError = errPipe
    process.standardInput = FileHandle.nullDevice
    do {
        try process.run()
    } catch {
        return RunResult(status: 127, out: "", err: "could not run make: \(error.localizedDescription)")
    }
    // gh can hang on the network; give up after two minutes.
    let timeout = DispatchWorkItem { if process.isRunning { process.terminate() } }
    DispatchQueue.global().asyncAfter(deadline: .now() + 120, execute: timeout)
    var errData = Data()
    let errRead = DispatchGroup()
    errRead.enter()
    DispatchQueue.global().async {
        errData = errPipe.fileHandleForReading.readDataToEndOfFile()
        errRead.leave()
    }
    let outData = outPipe.fileHandleForReading.readDataToEndOfFile()
    errRead.wait()
    process.waitUntilExit()
    timeout.cancel()
    return RunResult(status: process.terminationStatus,
                     out: String(decoding: outData, as: UTF8.self),
                     err: String(decoding: errData, as: UTF8.self))
}

/// Parses the `<url>  <title>` lines. Only github.com URLs are kept, since the URL is opened on click.
func parse(_ output: String) -> [Item] {
    output.split(separator: "\n").compactMap { line in
        guard let gap = line.range(of: "  ") else { return nil }
        let url = String(line[line.startIndex..<gap.lowerBound])
        guard url.hasPrefix("https://github.com/") else { return nil }
        return Item(url: url, title: String(line[gap.upperBound...]))
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private let statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    private let menu = NSMenu()
    private var cache: Cache?
    private var lastError: String?
    private var note: String?
    private var refreshing = false
    private var timer: Timer?

    func applicationDidFinishLaunching(_ notification: Notification) {
        loadCache()
        menu.delegate = self
        statusItem.menu = menu
        if let button = statusItem.button {
            button.image = NSImage(systemSymbolName: "arrow.triangle.pull", accessibilityDescription: "Pull requests ready for review")
            button.image?.isTemplate = true
            button.imagePosition = .imageLeading
            button.toolTip = "Pull requests ready for review"
        }
        updateTitle()
        timer = Timer.scheduledTimer(withTimeInterval: Settings.interval, repeats: true) { [weak self] _ in
            self?.refresh()
        }
        NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didWakeNotification, object: nil, queue: .main) { [weak self] _ in
                // The network is often not up yet at the moment of wake.
                DispatchQueue.main.asyncAfter(deadline: .now() + 10) { self?.refresh() }
            }
        refresh()
    }

    // MARK: cache

    private func loadCache() {
        guard let data = try? Data(contentsOf: Settings.cacheURL) else { return }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        cache = try? decoder.decode(Cache.self, from: data)
    }

    private func saveCache() {
        guard let cache else { return }
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        guard let data = try? encoder.encode(cache) else { return }
        let url = Settings.cacheURL
        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? data.write(to: url, options: .atomic)
    }

    // MARK: refresh

    @objc func refresh() {
        guard !refreshing else { return }
        refreshing = true
        DispatchQueue.global(qos: .utility).async { [weak self] in
            let result = runReviewReady()
            DispatchQueue.main.async { self?.finish(result) }
        }
    }

    private func finish(_ result: RunResult) {
        refreshing = false
        if result.status == 0 {
            cache = Cache(updatedAt: Date(), items: parse(result.out))
            saveCache()
            lastError = nil
            note = result.err.split(separator: "\n").first { $0.hasPrefix("incomplete:") }.map(String.init)
        } else {
            // Keep showing the cached list, marked as stale.
            let line = result.err.split(separator: "\n").last.map(String.init) ?? "exit \(result.status)"
            lastError = line
        }
        updateTitle()
    }

    private func updateTitle() {
        let count = cache?.items.count ?? 0
        var title = count > 0 ? " \(count)" : ""
        if statusItem.button?.image == nil { title = "PR" + title }
        if lastError != nil { title += "!" }
        statusItem.button?.title = title
    }

    // MARK: menu

    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        let items = cache?.items ?? []
        let heading = cache == nil
            ? (refreshing ? "Loading…" : "Not loaded yet")
            : items.isEmpty ? "No pull requests ready for review"
            : "\(items.count) ready for review"
        let header = NSMenuItem(title: heading, action: nil, keyEquivalent: "")
        header.isEnabled = false
        menu.addItem(header)
        for item in items {
            let title = item.title.count > 80 ? String(item.title.prefix(79)) + "…" : item.title
            let entry = NSMenuItem(title: "#\(item.number)  \(title)", action: #selector(open(_:)), keyEquivalent: "")
            entry.target = self
            entry.representedObject = item.url
            entry.toolTip = item.title
            menu.addItem(entry)
        }
        menu.addItem(.separator())
        if let lastError {
            let failure = NSMenuItem(title: "Last refresh failed: \(lastError.prefix(100))", action: nil, keyEquivalent: "")
            failure.isEnabled = false
            menu.addItem(failure)
        }
        if let note {
            let incomplete = NSMenuItem(title: String(note.prefix(100)), action: nil, keyEquivalent: "")
            incomplete.isEnabled = false
            menu.addItem(incomplete)
        }
        let status: String
        if refreshing {
            status = "Refreshing…"
        } else if let cache {
            let age = RelativeDateTimeFormatter()
            age.unitsStyle = .short
            status = "Updated " + age.localizedString(for: cache.updatedAt, relativeTo: Date())
        } else {
            status = "Never updated"
        }
        let updated = NSMenuItem(title: status, action: nil, keyEquivalent: "")
        updated.isEnabled = false
        menu.addItem(updated)
        let refreshItem = NSMenuItem(title: "Refresh Now", action: #selector(refresh), keyEquivalent: "r")
        refreshItem.target = self
        refreshItem.isEnabled = !refreshing
        menu.addItem(refreshItem)
        menu.addItem(.separator())
        menu.addItem(NSMenuItem(title: "Quit", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))
    }

    @objc func open(_ sender: NSMenuItem) {
        guard let string = sender.representedObject as? String, let url = URL(string: string) else { return }
        NSWorkspace.shared.open(url)
    }
}

let application = NSApplication.shared
application.setActivationPolicy(.accessory)
let delegate = AppDelegate()
application.delegate = delegate
application.run()
