# Mobile app shell

Status: built in `mobile/`. It builds and runs on the Android emulator and a device and on the iOS simulator, and shows a screen that says nothing is connected.

## Purpose

The shell is the React Native project and its build wiring. It settles the app's structure, its dependency cost and its behavior on a device before any target is connected. Screens for sessions, transcripts and approvals are added on top of it with the session client in `packages/agent-client`.

## Scope

In scope:

- One bare React Native app (see [app-dependencies.md](app-dependencies.md)) with an Android project and an iOS project, both named Bravebot with the id `com.brave.bravebot`.
- The app icon, cut from the desktop app's icon by `mobile/scripts/make-icons.sh`.
- The blank slate: the top bar, a conversation card and a disabled composer that says "No target is connected."
- `make check-mobile`, which type-checks and tests the app without a device or simulator, wired into CI and the affected-checks table.
- A link to `packages/agent-client`, with the Metro, Babel and Jest settings that link needs.

Out of scope: any session, target, host connection, pairing, notifications, persistence, markdown rendering, landscape and tablet layouts.

## Behavior

The app opens on the blank slate. It offers nothing to start. The composer is disabled, and the note above it gives the reason. The app is portrait only on both platforms.

A release build is signed only with a key supplied in Gradle properties. Without one it stays unsigned, so the template's public debug key never signs a release. No release or distribution build has been tested.

## Checks

`make check-mobile` builds the session client, installs the app's dependencies from the lockfile without running install scripts, then runs `tsc --noEmit` and Jest. A native build is not part of CI.
