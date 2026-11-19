// A menu bar item for maintainers that lists the pull requests `make review-ready` reports.
//
// The list is cached in ~/Library/Caches/bravebot-review-ready/cache.json and read at launch, so the
// menu has content as soon as the app starts and opens at once when clicked. `make review-ready`
// runs in the background on a timer, after the Mac wakes, and on "Refresh Now". The number of ready
// pull requests is shown next to the icon. The menu lists them in the order `make review-ready`
// prints them, the one that has waited longest first, and does not sort them again.
//
// The menu also has "Add N eligible to merge queue". A listed pull request is eligible when it has at
// least two approvals and no unresolved review thread. Each refresh asks `make review-ready` for the
// eligible ones in a file (ELIGIBLE_FILE), and the item is disabled while there are none. Choosing it
// runs `make review-ready ENQUEUE=1`, which reads the pull requests again and adds the eligible ones to
// the merge queue, so the count shown is not what decides what is added.
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
    /// The listed pull requests that are eligible for the merge queue. Absent in a cache written before
    /// the menu had the item, which counts as none until the next refresh.
    var eligible: [Item]?
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

    /// Where `make review-ready ELIGIBLE_FILE=` writes the eligible pull requests during a refresh.
    static var eligibleURL: URL {
        cacheURL.deletingLastPathComponent().appendingPathComponent("eligible.txt")
    }
}

struct RunResult {
    var status: Int32
    var out: String
    var err: String
}

/// Runs `make review-ready` in the checkout with `extra` make arguments after the configured ones. A menu
/// bar app does not inherit the shell's PATH, so the places gh and python3 are looked for are named in
/// Settings.searchPath. The process is ended after `seconds`, since gh can hang on the network.
func runReviewReady(extra: [String] = [], timeoutSeconds: Double = 120) -> RunResult {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/make")
    process.arguments = ["--no-print-directory", "-C", Settings.repoPath, "review-ready"]
        + Settings.makeArguments + extra
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
    // gh can hang on the network; give up after the timeout.
    let timeout = DispatchWorkItem { if process.isRunning { process.terminate() } }
    DispatchQueue.global().asyncAfter(deadline: .now() + timeoutSeconds, execute: timeout)
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
    private var queuedNote: String?
    /// Why the last "Add eligible to merge queue" did not add everything. The refresh that follows an add
    /// does not clear it, so the menu still names the refused pull request. It is cleared by the next add
    /// and by "Refresh Now".
    private var enqueueError: String?
    private var refreshing = false
    private var enqueuing = false
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

    /// The timer's and the wake-up's refresh. It leaves an add's refusal in the menu.
    @objc func refresh() {
        startRefresh(keepingQueuedNote: false)
    }

    /// "Refresh Now": a person asked for a new list, so an earlier add's refusal is dropped.
    @objc func refreshNow() {
        guard !refreshing, !enqueuing else { return }
        enqueueError = nil
        startRefresh(keepingQueuedNote: false)
    }

    private func startRefresh(keepingQueuedNote: Bool) {
        guard !refreshing, !enqueuing else { return }
        refreshing = true
        if !keepingQueuedNote { queuedNote = nil }
        // A file left by an earlier refresh must not be read as this one's answer.
        try? FileManager.default.createDirectory(at: Settings.eligibleURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? FileManager.default.removeItem(at: Settings.eligibleURL)
        DispatchQueue.global(qos: .utility).async { [weak self] in
            let result = runReviewReady(extra: ["ELIGIBLE_FILE=\(Settings.eligibleURL.path)"])
            DispatchQueue.main.async { self?.finish(result) }
        }
    }

    private func finish(_ result: RunResult) {
        refreshing = false
        if result.status == 0 {
            let eligible = (try? String(contentsOf: Settings.eligibleURL, encoding: .utf8)).map(parse) ?? []
            cache = Cache(updatedAt: Date(), items: parse(result.out), eligible: eligible)
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

    /// Adds the eligible pull requests to the merge queue, then reads the list again.
    @objc func addEligibleToMergeQueue() {
        guard !refreshing, !enqueuing, !(cache?.eligible ?? []).isEmpty else { return }
        enqueuing = true
        queuedNote = nil
        enqueueError = nil
        DispatchQueue.global(qos: .utility).async { [weak self] in
            // Reading the threads of each candidate and queuing each one takes longer than a refresh.
            let result = runReviewReady(extra: ["ENQUEUE=1"], timeoutSeconds: 300)
            DispatchQueue.main.async { self?.finishEnqueue(result) }
        }
    }

    private func finishEnqueue(_ result: RunResult) {
        enqueuing = false
        let added = parse(result.out).count
        if added > 0 {
            queuedNote = "Added \(added) to the merge queue"
        }
        if result.status != 0 {
            enqueueError = result.err.split(separator: "\n").last.map(String.init) ?? "exit \(result.status)"
        }
        updateTitle()
        startRefresh(keepingQueuedNote: true)
    }

    private func updateTitle() {
        let count = cache?.items.count ?? 0
        var title = count > 0 ? " \(count)" : ""
        if statusItem.button?.image == nil { title = "PR" + title }
        if lastError != nil || enqueueError != nil { title += "!" }
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
        let eligibleCount = cache?.eligible?.count ?? 0
        let queueItem = NSMenuItem(title: "Add \(eligibleCount) eligible to merge queue",
                                   action: #selector(addEligibleToMergeQueue), keyEquivalent: "")
        queueItem.target = self
        queueItem.isEnabled = eligibleCount > 0 && !refreshing && !enqueuing
        queueItem.toolTip = "Pull requests with at least two approvals, no unresolved review threads and passing checks"
        menu.addItem(queueItem)
        menu.addItem(.separator())
        if let queuedNote {
            let queued = NSMenuItem(title: queuedNote, action: nil, keyEquivalent: "")
            queued.isEnabled = false
            menu.addItem(queued)
        }
        if let enqueueError {
            let refused = NSMenuItem(title: "Not added to the merge queue: \(enqueueError.prefix(100))", action: nil, keyEquivalent: "")
            refused.isEnabled = false
            menu.addItem(refused)
        }
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
        if enqueuing {
            status = "Adding to the merge queue…"
        } else if refreshing {
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
        let refreshItem = NSMenuItem(title: "Refresh Now", action: #selector(refreshNow), keyEquivalent: "r")
        refreshItem.target = self
        refreshItem.isEnabled = !refreshing && !enqueuing
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
